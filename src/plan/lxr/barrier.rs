//! Read/Write barrier implementations.

use std::sync::atomic::AtomicUsize;

use atomic::Ordering;

use super::LXR;
use crate::plan::barriers::BarrierSemantics;
use crate::plan::barriers::LOGGED_VALUE;
use crate::plan::immix::Pause;
use crate::plan::lxr::cm::ProcessModBufSATB;
use crate::plan::lxr::rc::ProcessDecs;
use crate::plan::lxr::rc::ProcessIncs;
use crate::plan::lxr::rc::EDGE_KIND_MATURE;
use crate::plan::VectorQueue;
use crate::scheduler::gc_work::UnlogEdges;
use crate::scheduler::WorkBucketStage;
use crate::util::metadata::side_metadata::SideMetadataSpec;
use crate::util::rc::RC_LOCK_BITS;
use crate::util::*;
use crate::vm::edge_shape::Edge;
use crate::vm::edge_shape::MemorySlice;
use crate::vm::*;
use crate::LazySweepingJobsCounter;
use crate::MMTK;

pub const TAKERATE_MEASUREMENT: bool = crate::args::TAKERATE_MEASUREMENT;
pub static FAST_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static SLOW_COUNT: AtomicUsize = AtomicUsize::new(0);

pub const UNLOCKED_VALUE: u8 = 0b0;
pub const LOCKED_VALUE: u8 = 0b1;

pub struct LXRFieldBarrierSemantics<VM: VMBinding> {
    mmtk: &'static MMTK<VM>,
    edges: VectorQueue<Address>,
    incs: VectorQueue<VM::VMEdge>,
    decs: VectorQueue<ObjectReference>,
    refs: VectorQueue<ObjectReference>,
    lxr: &'static LXR<VM>,
}

impl<VM: VMBinding> LXRFieldBarrierSemantics<VM> {
    const UNLOG_BITS: SideMetadataSpec = *<VM as VMBinding>::VMObjectModel::GLOBAL_LOG_BIT_SPEC
        .as_spec()
        .extract_side_spec();
    const LOCK_BITS: SideMetadataSpec = RC_LOCK_BITS;

    #[allow(unused)]
    pub fn new(mmtk: &'static MMTK<VM>) -> Self {
        Self {
            mmtk,
            edges: VectorQueue::default(),
            incs: VectorQueue::default(),
            decs: VectorQueue::default(),
            refs: VectorQueue::default(),
            lxr: mmtk.plan.downcast_ref::<LXR<VM>>().unwrap(),
        }
    }

    #[inline(always)]
    fn get_edge_logging_state(&self, edge: Address) -> u8 {
        unsafe { Self::UNLOG_BITS.load(edge) }
    }

    #[inline(always)]
    fn attempt_to_lock_edge_bailout_if_logged(&self, edge: Address) -> bool {
        loop {
            // Bailout if logged
            if self.get_edge_logging_state(edge) == LOGGED_VALUE {
                return false;
            }
            // Attempt to lock the edges
            if Self::LOCK_BITS
                .compare_exchange_atomic(
                    edge,
                    UNLOCKED_VALUE,
                    LOCKED_VALUE,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                if self.get_edge_logging_state(edge) == LOGGED_VALUE {
                    self.unlock_edge(edge);
                    return false;
                }
                return true;
            }
            // Failed to lock the edge. Spin.
        }
    }

    #[inline(always)]
    fn unlock_edge(&self, edge: Address) {
        RC_LOCK_BITS.store_atomic(edge, UNLOCKED_VALUE, Ordering::Relaxed);
    }

    #[inline(always)]
    fn log_and_unlock_edge(&self, edge: Address) {
        if (1 << crate::args::LOG_BYTES_PER_RC_LOCK_BIT) >= 64 {
            unsafe { Self::UNLOG_BITS.store(edge, LOGGED_VALUE) };
        } else {
            Self::UNLOG_BITS.store_atomic(edge, LOGGED_VALUE, Ordering::Relaxed);
        }
        RC_LOCK_BITS.store_atomic(edge, UNLOCKED_VALUE, Ordering::Relaxed);
    }

    #[inline(always)]
    fn log_edge_and_get_old_target(&self, edge: Address) -> Result<ObjectReference, ()> {
        if self.attempt_to_lock_edge_bailout_if_logged(edge) {
            let old: ObjectReference = unsafe { edge.load() };
            self.log_and_unlock_edge(edge);
            Ok(old)
        } else {
            Err(())
        }
    }

    #[inline(always)]
    #[allow(unused)]
    fn log_edge_and_get_old_target_sloppy(&self, edge: Address) -> Result<ObjectReference, ()> {
        if !edge.is_logged::<VM>() {
            let old: ObjectReference = unsafe { edge.load() };
            edge.log::<VM>();
            Ok(old)
        } else {
            Err(())
        }
    }

    #[inline(always)]
    fn slow(&mut self, _src: ObjectReference, edge: VM::VMEdge, old: ObjectReference) {
        #[cfg(any(feature = "sanity", debug_assertions))]
        assert!(
            old.is_null() || crate::util::rc::count(old) != 0,
            "zero rc count {:?}",
            old
        );
        // Concurrent Marking
        // if !crate::args::REF_COUNT && self.lxr.concurrent_marking_in_progress() {
        //     self.edges.push(edge);
        //     if !old.is_null() {
        //         self.nodes.push(old);
        //     }
        // }
        // Reference counting
        // if crate::args::BARRIER_MEASUREMENT || crate::args::REF_COUNT {
        if !old.is_null() {
            self.decs.push(old);
        }
        self.incs.push(edge);
        crate::util::rc::inc_inc_buffer_size();
        // }
        // Flush
        if self.edges.is_full() || self.incs.is_full() || self.decs.is_full() {
            self.flush();
        }
    }

    #[inline(always)]
    fn enqueue_node(
        &mut self,
        src: ObjectReference,
        edge: VM::VMEdge,
        _new: Option<ObjectReference>,
    ) {
        if TAKERATE_MEASUREMENT && self.mmtk.inside_harness() {
            FAST_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        if let Ok(old) = self.log_edge_and_get_old_target(edge.to_address()) {
            if TAKERATE_MEASUREMENT && self.mmtk.inside_harness() {
                SLOW_COUNT.fetch_add(1, Ordering::SeqCst);
            }
            self.slow(src, edge, old)
        }
    }
}

impl<VM: VMBinding> BarrierSemantics for LXRFieldBarrierSemantics<VM> {
    type VM = VM;

    #[cold]
    fn flush(&mut self) {
        // Barrier measurement: simply unlog remembered edges
        if crate::args::BARRIER_MEASUREMENT {
            // Unlog inc edges
            if !self.incs.is_empty() {
                let _decs = self.decs.take();
                let incs = self.incs.take();
                self.mmtk.scheduler.work_buckets[WorkBucketStage::CalculateForwarding]
                    .add(UnlogEdges::new(incs));
            }
            return;
        }
        // Concurrent Marking: Flush satb buffer
        #[allow(clippy::collapsible_if)]
        if self.lxr.concurrent_marking_enabled()
            && (self.lxr.concurrent_marking_in_progress()
                || self.lxr.current_pause() == Some(Pause::FinalMark))
        {
            if !self.decs.is_empty() {
                let nodes = self.decs.clone_buffer();
                self.mmtk.scheduler.work_buckets[WorkBucketStage::Initial]
                    .add(ProcessModBufSATB::<VM>::new(nodes));
            }
            if !self.refs.is_empty() {
                let nodes = self.refs.take();
                self.mmtk.scheduler.work_buckets[WorkBucketStage::Initial]
                    .add(ProcessModBufSATB::<VM>::new(nodes));
            }
        } else {
            self.refs.clear()
        }
        // Flush inc and dec buffer
        if !self.incs.is_empty() {
            // Inc buffer
            let incs = self.incs.take();
            let bucket = WorkBucketStage::rc_process_incs_stage();
            self.mmtk.scheduler.work_buckets[bucket]
                .add(ProcessIncs::<_, { EDGE_KIND_MATURE }>::new(incs));
            // Dec buffer
            let decs = self.decs.take();
            let w = ProcessDecs::new(decs, LazySweepingJobsCounter::new_desc());
            if crate::args::LAZY_DECREMENTS && !crate::args::BARRIER_MEASUREMENT {
                self.mmtk.scheduler.postpone_prioritized(w);
            } else {
                self.mmtk.scheduler.work_buckets[WorkBucketStage::RCProcessDecs].add(w);
            }
        }
    }

    fn object_reference_write_slow(
        &mut self,
        _src: ObjectReference,
        slot: VM::VMEdge,
        _target: ObjectReference,
    ) {
        self.enqueue_node(ObjectReference::NULL, slot, None);
    }

    fn memory_region_copy_slow(&mut self, _src: VM::VMMemorySlice, dst: VM::VMMemorySlice) {
        for e in dst.iter_edges() {
            self.enqueue_node(ObjectReference::NULL, e, None);
        }
    }

    fn load_reference(&mut self, o: ObjectReference) {
        self.refs.push(o);
        if self.refs.is_full() {
            self.flush();
        }
    }
}