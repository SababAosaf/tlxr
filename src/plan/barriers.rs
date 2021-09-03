//! Read/Write barrier implementations.

use atomic::Ordering;

use crate::scheduler::gc_work::*;
use crate::scheduler::WorkBucketStage;
use crate::util::metadata::load_metadata;
use crate::util::metadata::{compare_exchange_metadata, MetadataSpec};
use crate::util::*;
use crate::vm::VMBinding;
use crate::MMTK;
use crate::util::metadata::side_metadata;
use crate::plan::immix::RC;

/// BarrierSelector describes which barrier to use.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BarrierSelector {
    NoBarrier,
    ObjectBarrier,
    FieldLoggingBarrier,
}

/// For field writes in HotSpot, we cannot always get the source object pointer and the field address\
#[derive(Debug)]
pub enum WriteTarget {
    Field {
        src: ObjectReference,
        slot: Address,
        val: ObjectReference,
    },
    ArrayCopy {
        src: ObjectReference,
        src_offset: usize,
        dst: ObjectReference,
        dst_offset: usize,
        len: usize,
    },
    Clone {
        src: ObjectReference,
        dst: ObjectReference,
    },
}

pub trait Barrier: 'static + Send {
    fn flush(&mut self);
    fn write_barrier(&mut self, target: WriteTarget);
}

pub struct NoBarrier;

impl Barrier for NoBarrier {
    fn flush(&mut self) {}
    fn write_barrier(&mut self, _target: WriteTarget) {
        unreachable!("write_barrier called on NoBarrier")
    }
}

pub struct ObjectRememberingBarrier<E: ProcessEdgesWork> {
    mmtk: &'static MMTK<E::VM>,
    modbuf: Vec<ObjectReference>,
    /// The metadata used for log bit. Though this allows taking an arbitrary metadata spec,
    /// for this field, 0 means logged, and 1 means unlogged (the same as the vm::object_model::VMGlobalLogBitSpec).
    meta: MetadataSpec,
}

impl<E: ProcessEdgesWork> ObjectRememberingBarrier<E> {
    #[allow(unused)]
    pub fn new(mmtk: &'static MMTK<E::VM>, meta: MetadataSpec) -> Self {
        Self {
            mmtk,
            modbuf: vec![],
            meta,
        }
    }

    /// Attepmt to atomically log an object.
    /// Returns true if the object is not logged previously.
    #[inline(always)]
    fn log_object(&self, object: ObjectReference) -> bool {
        loop {
            let old_value =
                load_metadata::<E::VM>(&self.meta, object, None, Some(Ordering::SeqCst));
            if old_value == 0 {
                return false;
            }
            if compare_exchange_metadata::<E::VM>(
                &self.meta,
                object,
                1,
                0,
                None,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                return true;
            }
        }
    }

    #[inline(always)]
    fn enqueue_node(&mut self, obj: ObjectReference) {
        // If the objecct is unlogged, log it and push it to mod buffer
        if self.log_object(obj) {
            self.modbuf.push(obj);
            if self.modbuf.len() >= E::CAPACITY {
                self.flush();
            }
        }
    }
}

impl<E: ProcessEdgesWork> Barrier for ObjectRememberingBarrier<E> {
    #[cold]
    fn flush(&mut self) {
        if self.modbuf.is_empty() {
            return;
        }
        let mut modbuf = vec![];
        std::mem::swap(&mut modbuf, &mut self.modbuf);
        debug_assert!(
            !self.mmtk.scheduler.work_buckets[WorkBucketStage::Final].is_activated(),
            "{:?}",
            self as *const _
        );
        if !modbuf.is_empty() {
            self.mmtk.scheduler.work_buckets[WorkBucketStage::Closure]
                .add(ProcessModBuf::<E>::new(modbuf, self.meta));
        }
    }

    #[inline(always)]
    fn write_barrier(&mut self, target: WriteTarget) {
        match target {
            WriteTarget::Field { src, .. } => {
                self.enqueue_node(src);
            }
            WriteTarget::ArrayCopy { dst, .. } => {
                self.enqueue_node(dst);
            }
            WriteTarget::Clone { dst, .. } => {
                self.enqueue_node(dst);
            }
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum FLBKind {
    SATB,
    IU,
}

pub struct FieldLoggingBarrier<E: ProcessEdgesWork, const KIND: FLBKind> {
    mmtk: &'static MMTK<E::VM>,
    edges: Vec<Address>,
    nodes: Vec<ObjectReference>,
    /// The metadata used for log bit. Though this allows taking an arbitrary metadata spec,
    /// for this field, 0 means logged, and 1 means unlogged (the same as the vm::object_model::VMGlobalLogBitSpec).
    meta: MetadataSpec,
}

impl<E: ProcessEdgesWork, const KIND: FLBKind> FieldLoggingBarrier<E, KIND> {
    #[allow(unused)]
    pub fn new(mmtk: &'static MMTK<E::VM>, meta: MetadataSpec) -> Self {
        Self {
            mmtk,
            edges: vec![],
            nodes: vec![],
            meta,
        }
    }

    #[inline(always)]
    fn log_edge(&self, edge: Address) -> bool {
        loop {
            let old_value = load_metadata::<E::VM>(
                &self.meta,
                unsafe { edge.to_object_reference() },
                None,
                Some(Ordering::SeqCst),
            );
            if old_value == 1 {
                return false;
            }
            if compare_exchange_metadata::<E::VM>(
                &self.meta,
                unsafe { edge.to_object_reference() },
                0,
                1,
                None,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                return true;
            }
        }
    }

    #[inline(always)]
    fn enqueue_node(&mut self, edge: Address) {
        if !crate::plan::immix::BARRIER_MEASUREMENT && !*crate::IN_CONCURRENT_GC.lock() {
            return;
        }
        if self.log_edge(edge) {
            self.edges.push(edge);
            if KIND == FLBKind::SATB {
                let node: ObjectReference = unsafe { edge.load() };
                if !node.is_null() {
                    self.nodes.push(node);
                }
            }
            if self.edges.len() >= E::CAPACITY {
                self.flush();
            }
        }
    }

    #[inline(always)]
    fn inc(&mut self, o: ObjectReference) {
        if o.is_null() { return }
        let _ = side_metadata::fetch_update(RC.extract_side_spec(), o.to_address(), Ordering::SeqCst, Ordering::SeqCst, |x| {
            if x == 0b1111 {
                None
            } else {
                Some(x + 1)
            }
        });
    }

    #[inline(always)]
    fn dec(&mut self, o: ObjectReference) {
        if o.is_null() { return }
        let res = side_metadata::fetch_update(RC.extract_side_spec(), o.to_address(), Ordering::SeqCst, Ordering::SeqCst, |x| {
            if x == 0 || x == 0b1111 /* sticky */ {
                None
            } else {
                Some(x - 1)
            }
        });
        if res == Ok(1) {
            // Release this one
            println!("Release {:?}", o);
        }
    }
}

impl<E: ProcessEdgesWork, const KIND: FLBKind> Barrier for FieldLoggingBarrier<E, KIND> {
    #[cold]
    fn flush(&mut self) {
        if KIND == FLBKind::SATB {
            if self.edges.is_empty() && self.nodes.is_empty() {
                return;
            }
            let mut edges = vec![];
            std::mem::swap(&mut edges, &mut self.edges);
            let mut nodes = vec![];
            std::mem::swap(&mut nodes, &mut self.nodes);
            self.mmtk.scheduler.work_buckets[WorkBucketStage::RefClosure]
                .add(ProcessModBufSATB::<E>::new(edges, nodes, self.meta));
        } else {
            if self.edges.is_empty() {
                return;
            }
            let mut edges = vec![];
            std::mem::swap(&mut edges, &mut self.edges);
            self.mmtk.scheduler.work_buckets[WorkBucketStage::RefClosure]
                .add(ProcessModBufIU::<E>::new(edges, self.meta));
        }
    }

    #[inline(always)]
    fn write_barrier(&mut self, target: WriteTarget) {
        match target {
            WriteTarget::Field { slot, val, .. } => {
                self.dec(unsafe { slot.load() });
                self.inc(val);
                self.enqueue_node(slot);
            }
            WriteTarget::ArrayCopy {
                src,
                src_offset,
                dst,
                dst_offset,
                len,
                ..
            } => {
                let src_base = src.to_address() + src_offset;
                let dst_base = dst.to_address() + dst_offset;
                for i in 0..len {
                    self.dec(unsafe { (dst_base + (i << 3)).load() });
                    self.inc(unsafe { (src_base + (i << 3)).load() });
                    self.enqueue_node(src_base + (i << 3));
                }
            }
            WriteTarget::Clone { .. } => {
                // How to deal with this?
            }
        }
    }
}
