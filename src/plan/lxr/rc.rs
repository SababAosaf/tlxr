use super::cm::LXRConcurrentTraceObjects;
use super::cm::LXRStopTheWorldProcessEdges;
use super::LXR;
use crate::plan::VectorQueue;
use crate::policy::immix::block::BlockState;
use crate::scheduler::gc_work::EdgeOf;
use crate::scheduler::gc_work::ScanObjects;
use crate::util::address::CLDScanPolicy;
use crate::util::address::RefScanPolicy;
use crate::util::copy::CopySemantics;
use crate::util::copy::GCWorkerCopyContext;
use crate::util::metadata::side_metadata::SideMetadataSpec;
use crate::util::rc::*;
use crate::vm::edge_shape::Edge;
use crate::vm::edge_shape::MemorySlice;
use crate::LazySweepingJobsCounter;
use crate::{
    plan::immix::Pause,
    policy::{immix::block::Block, space::Space},
    scheduler::{gc_work::ProcessEdgesBase, GCWork, GCWorker, ProcessEdgesWork, WorkBucketStage},
    util::{metadata::side_metadata, object_forwarding, ObjectReference},
    vm::*,
    MMTK,
};
use atomic::Ordering;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

pub struct ProcessIncs<VM: VMBinding, const KIND: EdgeKind, const COMPRESSED: bool> {
    /// Increments to process
    incs: Vec<VM::VMEdge>,
    /// Recursively generated new increments
    new_incs: VectorQueue<VM::VMEdge>,
    lxr: *const LXR<VM>,
    current_pause: Pause,
    concurrent_marking_in_progress: bool,
    no_evac: bool,
    slice: Option<VM::VMMemorySlice>,
    depth: usize,
    rc: RefCountHelper<VM>,
}

unsafe impl<VM: VMBinding, const KIND: EdgeKind, const COMPRESSED: bool> Send
    for ProcessIncs<VM, KIND, COMPRESSED>
{
}

impl<VM: VMBinding, const KIND: EdgeKind, const COMPRESSED: bool>
    ProcessIncs<VM, KIND, COMPRESSED>
{
    const CAPACITY: usize = crate::args::BUFFER_SIZE;
    const UNLOG_BITS: SideMetadataSpec = crate::policy::immix::UnlogBit::<VM, COMPRESSED>::SPEC;

    #[inline(always)]
    fn worker(&self) -> &'static mut GCWorker<VM> {
        GCWorker::<VM>::current()
    }

    #[inline(always)]
    fn lxr(&self) -> &'static LXR<VM> {
        unsafe { &*self.lxr }
    }

    #[inline]
    pub fn new_array_slice(slice: VM::VMMemorySlice) -> Self {
        Self {
            incs: vec![],
            new_incs: VectorQueue::default(),
            lxr: std::ptr::null(),
            current_pause: Pause::RefCount,
            concurrent_marking_in_progress: false,
            no_evac: false,
            slice: Some(slice),
            depth: 1,
            rc: RefCountHelper::NEW,
        }
    }

    #[inline]
    pub fn new(incs: Vec<VM::VMEdge>) -> Self {
        Self {
            incs,
            new_incs: VectorQueue::default(),
            lxr: std::ptr::null(),
            current_pause: Pause::RefCount,
            concurrent_marking_in_progress: false,
            no_evac: false,
            slice: None,
            depth: 1,
            rc: RefCountHelper::NEW,
        }
    }

    #[inline(always)]
    fn promote(&mut self, o: ObjectReference, copied: bool, los: bool, depth: usize) {
        o.verify::<VM>();
        crate::stat(|s| {
            s.promoted_objects += 1;
            s.promoted_volume += o.get_size::<VM>();
            if self.lxr().los().in_space(o) {
                s.promoted_los_objects += 1;
                s.promoted_los_volume += o.get_size::<VM>();
            }
            if copied {
                s.promoted_copy_objects += 1;
                s.promoted_copy_volume += o.get_size::<VM>();
            }
        });
        if !los {
            if !copied && Block::containing::<VM>(o).get_state() == BlockState::Nursery {
                Block::containing::<VM>(o).set_as_in_place_promoted();
            }
            self.rc.promote::<COMPRESSED>(o);
            crate::plan::lxr::SURVIVAL_RATIO_PREDICTOR_LOCAL
                .with(|x| x.record_promotion(o.get_size::<VM>()));
        } else {
            // println!("promote los {:?} {}", o, self.immix().is_marked(o));
        }
        // Don't mark copied objects in initial mark pause. The concurrent marker will do it (and can also resursively mark the old objects).
        if self.concurrent_marking_in_progress || self.current_pause == Pause::FinalMark {
            self.lxr().mark2(o, los);
        }
        self.scan_nursery_object(o, los, !copied, depth);
    }

    #[inline(always)]
    fn record_mature_evac_remset(&mut self, e: VM::VMEdge, o: ObjectReference, force: bool) {
        if !(crate::args::RC_MATURE_EVACUATION
            && (self.concurrent_marking_in_progress || self.current_pause == Pause::FinalMark))
        {
            return;
        }
        if force || (!self.lxr().address_in_defrag(e.to_address()) && self.lxr().in_defrag(o)) {
            self.lxr()
                .immix_space
                .remset
                .record(e, &self.lxr().immix_space);
        }
    }

    #[inline(always)]
    fn scan_nursery_object(
        &mut self,
        o: ObjectReference,
        los: bool,
        in_place_promotion: bool,
        depth: usize,
    ) {
        let heap_bytes_per_unlog_byte = if COMPRESSED { 32usize } else { 64 };
        let heap_bytes_per_unlog_bit = if COMPRESSED { 4usize } else { 8 };
        if los {
            if !VM::VMScanning::is_val_array::<COMPRESSED>(o) {
                let start =
                    side_metadata::address_to_meta_address(&Self::UNLOG_BITS, o.to_address::<VM>())
                        .to_mut_ptr::<u8>();
                let limit = side_metadata::address_to_meta_address(
                    &Self::UNLOG_BITS,
                    (o.to_address::<VM>() + o.get_size::<VM>()).align_up(heap_bytes_per_unlog_byte),
                )
                .to_mut_ptr::<u8>();
                let bytes = unsafe { limit.offset_from(start) as usize };
                unsafe {
                    std::ptr::write_bytes(start, 0xffu8, bytes);
                }
            } else {
                o.to_address::<VM>().unlog_non_atomic::<VM, COMPRESSED>();
                (o.to_address::<VM>() + 8usize).unlog_non_atomic::<VM, COMPRESSED>();
            }
        } else if in_place_promotion {
            let header_size = if COMPRESSED { 12usize } else { 16 };
            let size = o.get_size::<VM>();
            let end = o.to_address::<VM>() + size;
            let aligned_end = end.align_down(heap_bytes_per_unlog_byte);
            let mut cursor = o.to_address::<VM>() + header_size;
            let mut meta = side_metadata::address_to_meta_address(
                &Self::UNLOG_BITS,
                cursor.align_up(heap_bytes_per_unlog_byte),
            );
            while cursor < end && !cursor.is_aligned_to(heap_bytes_per_unlog_byte) {
                cursor.unlog::<VM, COMPRESSED>();
                cursor += heap_bytes_per_unlog_bit;
            }
            while cursor < aligned_end {
                if cursor.is_aligned_to(heap_bytes_per_unlog_byte) {
                    unsafe { meta.store(0xffu8) }
                    meta += 1usize;
                    cursor += heap_bytes_per_unlog_byte;
                } else {
                    cursor.unlog::<VM, COMPRESSED>();
                    cursor += heap_bytes_per_unlog_bit;
                }
            }
            while cursor < end {
                cursor.unlog::<VM, COMPRESSED>();
                cursor += heap_bytes_per_unlog_bit;
            }
        };
        if VM::VMScanning::is_obj_array::<COMPRESSED>(o)
            && VM::VMScanning::obj_array_data::<COMPRESSED>(o).len() >= 1024
        {
            let data = VM::VMScanning::obj_array_data::<COMPRESSED>(o);
            let mut packets = vec![];
            for chunk in data.chunks(Self::CAPACITY) {
                let mut w = Box::new(
                    ProcessIncs::<VM, EDGE_KIND_NURSERY, COMPRESSED>::new_array_slice(chunk),
                );
                w.depth = depth + 1;
                packets.push(w as Box<dyn GCWork<VM>>);
            }
            self.worker().scheduler().work_buckets[WorkBucketStage::Unconstrained]
                .bulk_add(packets);
        } else {
            o.iterate_fields::<VM, _, COMPRESSED>(
                if self.current_pause != Pause::FullTraceFast {
                    CLDScanPolicy::Claim
                } else {
                    CLDScanPolicy::Follow
                },
                RefScanPolicy::Follow,
                |edge| {
                    let target = edge.load::<COMPRESSED>();
                    // println!(" -- rec inc opt {:?}.{:?} -> {:?}", o, edge, target);
                    if !target.is_null() {
                        if !self.rc.is_stuck(target) {
                            // println!(" -- rec inc {:?}.{:?} -> {:?}", o, edge, target);
                            self.new_incs.push(edge);
                            if self.new_incs.is_full() {
                                self.flush()
                            }
                        } else {
                            super::record_edge_for_validation(edge, target);
                            self.record_mature_evac_remset(edge, target, false);
                        }
                    } else {
                        super::record_edge_for_validation(edge, target);
                    }
                },
            );
        }
    }

    #[cold]
    fn flush(&mut self) {
        if !self.new_incs.is_empty() {
            let new_incs = self.new_incs.take();
            let mut w = ProcessIncs::<VM, EDGE_KIND_NURSERY, COMPRESSED>::new(new_incs);
            w.depth += 1;
            self.worker().add_work(WorkBucketStage::Unconstrained, w);
        }
    }

    #[inline(always)]
    fn process_inc(&mut self, o: ObjectReference, depth: usize) -> ObjectReference {
        if let Ok(0) = self.rc.inc(o) {
            self.promote(o, false, self.lxr().los().in_space(o), depth);
        }
        o
    }

    #[inline(always)]
    fn dont_evacuate(&self, o: ObjectReference, los: bool) -> bool {
        if los {
            return true;
        }
        // Skip mature object
        if self.rc.count(o) != 0 {
            return true;
        }
        // Skip recycled lines
        if crate::args::RC_DONT_EVACUATE_NURSERY_IN_RECYCLED_LINES
            && Block::containing::<VM>(o).get_state() == BlockState::Reusing
        {
            return true;
        }
        if o.get_size::<VM>() >= crate::args().max_copy_size {
            return true;
        }
        false
    }

    #[inline(always)]
    fn process_inc_and_evacuate(
        &mut self,
        o: ObjectReference,
        copy_context: &mut GCWorkerCopyContext<VM>,
        depth: usize,
    ) -> ObjectReference {
        o.verify::<VM>();
        crate::stat(|s| {
            s.inc_objects += 1;
            s.inc_volume += o.get_size::<VM>();
        });
        debug_assert!(crate::args::RC_NURSERY_EVACUATION);
        let los = self.lxr().los().in_space(o);
        if self.dont_evacuate(o, los) {
            if let Ok(0) = self.rc.inc(o) {
                self.promote(o, false, los, depth);
            }
            return o;
        }
        if object_forwarding::is_forwarded::<VM>(o) {
            let new = object_forwarding::read_forwarding_pointer::<VM>(o);
            let _ = self.rc.inc(new);
            return new;
        }
        let forwarding_status = object_forwarding::attempt_to_forward::<VM>(o);
        if object_forwarding::state_is_forwarded_or_being_forwarded(forwarding_status) {
            // Object is moved to a new location.
            let new = object_forwarding::spin_and_get_forwarded_object::<VM>(o, forwarding_status);
            let _ = self.rc.inc(new);
            new
        } else {
            let is_nursery = self.rc.count(o) == 0;
            let copy_depth_reached = crate::args::INC_MAX_COPY_DEPTH && depth > 16;
            if is_nursery && !self.no_evac && !copy_depth_reached {
                // Evacuate the object
                let new = object_forwarding::forward_object::<VM>(
                    o,
                    CopySemantics::DefaultCopy,
                    copy_context,
                );
                if crate::should_record_copy_bytes() {
                    unsafe { crate::SLOPPY_COPY_BYTES += new.get_size::<VM>() }
                }
                let _ = self.rc.inc(new);
                self.promote(new, true, false, depth);
                new
            } else {
                // Object is not moved.
                let r = self.rc.inc(o);
                object_forwarding::clear_forwarding_bits::<VM>(o);
                if let Ok(0) = r {
                    self.promote(o, false, los, depth);
                }
                o
            }
        }
    }

    /// Return `None` if the increment of the edge should be delayed
    #[inline(always)]
    fn unlog_and_load_rc_object<const K: EdgeKind>(
        &mut self,
        e: VM::VMEdge,
    ) -> Option<ObjectReference> {
        debug_assert!(!crate::args::EAGER_INCREMENTS);
        let o = e.load::<COMPRESSED>();
        // unlog edge
        if K == EDGE_KIND_MATURE {
            e.to_address().unlog::<VM, COMPRESSED>();
        }
        if o.is_null() {
            return None;
        }
        Some(o)
    }

    #[inline(always)]
    fn process_edge<const K: EdgeKind>(
        &mut self,
        e: VM::VMEdge,
        cc: &mut GCWorkerCopyContext<VM>,
        depth: usize,
    ) -> Option<ObjectReference> {
        let o = match self.unlog_and_load_rc_object::<K>(e) {
            Some(o) => o,
            _ => {
                super::record_edge_for_validation(e, ObjectReference::NULL);
                return None;
            }
        };
        // println!(" - inc {:?}: {:?} rc={}", e, o, count(o));
        o.verify::<VM>();
        let new = if !crate::args::RC_NURSERY_EVACUATION {
            self.process_inc(o, depth)
        } else {
            self.process_inc_and_evacuate(o, cc, depth)
        };
        if K != EDGE_KIND_ROOT {
            self.record_mature_evac_remset(e, o, false);
        }
        if new != o {
            // println!(
            //     " -- inc {:?}: {:?} => {:?} rc={} {:?}",
            //     e,
            //     o,
            //     new.range::<VM>(),
            //     self.rc.count(new),
            //     K
            // );
            e.store::<COMPRESSED>(new)
        } else {
            // println!(
            //     " -- inc {:?}: {:?} rc={} {:?}",
            //     e,
            //     o.range::<VM>(),
            //     self.rc.count(o),
            //     K
            // );
        }
        super::record_edge_for_validation(e, new);
        Some(new)
    }

    #[inline(always)]
    fn process_incs<const K: EdgeKind>(
        &mut self,
        mut incs: AddressBuffer<'_, VM::VMEdge>,
        copy_context: &mut GCWorkerCopyContext<VM>,
        depth: usize,
    ) -> Option<Vec<ObjectReference>> {
        if K == EDGE_KIND_ROOT {
            let roots = incs.as_mut_ptr() as *mut ObjectReference;
            let mut num_roots = 0usize;
            for e in &mut *incs {
                if let Some(new) = self.process_edge::<K>(*e, copy_context, depth) {
                    unsafe {
                        roots.add(num_roots).write(new);
                    }
                    num_roots += 1;
                }
            }
            if num_roots != 0 {
                let cap = incs.capacity();
                std::mem::forget(incs);
                let roots =
                    unsafe { Vec::<ObjectReference>::from_raw_parts(roots, num_roots, cap) };
                Some(roots)
            } else {
                None
            }
        } else {
            for e in &mut *incs {
                self.process_edge::<K>(*e, copy_context, depth);
            }
            None
        }
    }

    #[inline(always)]
    fn process_incs_for_obj_array<const K: EdgeKind>(
        &mut self,
        slice: VM::VMMemorySlice,
        copy_context: &mut GCWorkerCopyContext<VM>,
        depth: usize,
    ) -> Option<Vec<ObjectReference>> {
        for e in slice.iter_edges() {
            self.process_edge::<K>(e, copy_context, depth);
        }
        None
    }
}

pub type EdgeKind = u8;
pub const EDGE_KIND_ROOT: u8 = 0;
pub const EDGE_KIND_NURSERY: u8 = 1;
pub const EDGE_KIND_MATURE: u8 = 2;

enum AddressBuffer<'a, E: Edge> {
    Owned(Vec<E>),
    Ref(&'a mut Vec<E>),
}

impl<E: Edge> Deref for AddressBuffer<'_, E> {
    type Target = Vec<E>;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(x) => x,
            Self::Ref(x) => x,
        }
    }
}

impl<E: Edge> DerefMut for AddressBuffer<'_, E> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Owned(x) => x,
            Self::Ref(x) => x,
        }
    }
}

impl<VM: VMBinding, const KIND: EdgeKind, const COMPRESSED: bool> GCWork<VM>
    for ProcessIncs<VM, KIND, COMPRESSED>
{
    #[inline(always)]
    fn do_work(&mut self, worker: &mut GCWorker<VM>, mmtk: &'static MMTK<VM>) {
        debug_assert!(!crate::plan::barriers::BARRIER_MEASUREMENT);
        self.lxr = mmtk.plan.downcast_ref::<LXR<VM>>().unwrap();
        self.current_pause = self.lxr().current_pause().unwrap();
        self.concurrent_marking_in_progress = self.lxr().concurrent_marking_in_progress();
        let copy_context = self.worker().get_copy_context_mut();
        if crate::NO_EVAC.load(Ordering::Relaxed) {
            self.no_evac = true;
        } else {
            let over_time = crate::args()
                .max_pause_millis
                .map(|threshold| {
                    crate::GC_START_TIME
                        .load(Ordering::Relaxed)
                        .elapsed()
                        .unwrap()
                        .as_millis()
                        >= threshold as u128
                })
                .unwrap_or(false);
            let over_space = mmtk.plan.get_used_pages() - mmtk.plan.get_collection_reserved_pages()
                > mmtk.plan.get_total_pages();
            if over_space || over_time {
                self.no_evac = true;
                crate::NO_EVAC.store(true, Ordering::Relaxed);
                if crate::args::LOG_PER_GC_STATE {
                    println!(
                        " - no evac over_space={} over_time={}",
                        over_space, over_time
                    );
                }
            }
        }
        // Process main buffer
        let root_edges = if KIND == EDGE_KIND_ROOT
            && (self.current_pause == Pause::FinalMark
                || self.current_pause == Pause::FullTraceFast)
        {
            self.incs.clone()
        } else {
            vec![]
        };
        let roots = {
            if let Some(slice) = self.slice.take() {
                assert_eq!(KIND, EDGE_KIND_NURSERY);
                self.process_incs_for_obj_array::<KIND>(slice, copy_context, self.depth)
            } else {
                let incs = std::mem::take(&mut self.incs);
                self.process_incs::<KIND>(AddressBuffer::Owned(incs), copy_context, self.depth)
            }
        };
        if let Some(roots) = roots {
            if self.lxr().concurrent_marking_enabled() && self.current_pause == Pause::InitialMark {
                worker
                    .scheduler()
                    .postpone(LXRConcurrentTraceObjects::<VM, COMPRESSED>::new(
                        roots.clone(),
                        mmtk,
                    ));
            }
            if self.current_pause == Pause::FinalMark || self.current_pause == Pause::FullTraceFast
            {
                worker.add_work(
                    WorkBucketStage::Closure,
                    LXRStopTheWorldProcessEdges::<VM, COMPRESSED>::new(root_edges, true, mmtk),
                )
            } else {
                unsafe {
                    crate::plan::lxr::CURR_ROOTS.push(roots);
                }
            }
        }
        // Process recursively generated buffer
        let mut depth = self.depth;
        let mut incs = vec![];
        while !self.new_incs.is_empty() {
            depth += 1;
            incs.clear();
            self.new_incs.swap(&mut incs);
            self.process_incs::<EDGE_KIND_NURSERY>(
                AddressBuffer::Ref(&mut incs),
                copy_context,
                depth,
            );
        }
        crate::plan::lxr::SURVIVAL_RATIO_PREDICTOR_LOCAL.with(|x| x.sync())
    }
}

pub struct ProcessDecs<VM: VMBinding, const COMPRESSED: bool> {
    /// Decrements to process
    decs: Option<Vec<ObjectReference>>,
    decs_arc: Option<Arc<Vec<ObjectReference>>>,
    slice: Option<(bool, &'static [ObjectReference])>,
    /// Recursively generated new decrements
    new_decs: VectorQueue<ObjectReference>,
    mmtk: *const MMTK<VM>,
    counter: LazySweepingJobsCounter,
    mark_objects: VectorQueue<ObjectReference>,
    concurrent_marking_in_progress: bool,
    mature_sweeping_in_progress: bool,
    rc: RefCountHelper<VM>,
}

unsafe impl<VM: VMBinding, const COMPRESSED: bool> Send for ProcessDecs<VM, COMPRESSED> {}

impl<VM: VMBinding, const COMPRESSED: bool> ProcessDecs<VM, COMPRESSED> {
    pub const CAPACITY: usize = crate::args::BUFFER_SIZE;

    #[inline(always)]
    fn worker(&self) -> &mut GCWorker<VM> {
        GCWorker::<VM>::current()
    }

    #[inline]
    pub fn new(decs: Vec<ObjectReference>, counter: LazySweepingJobsCounter) -> Self {
        Self {
            decs: Some(decs),
            decs_arc: None,
            slice: None,
            new_decs: VectorQueue::default(),
            mmtk: std::ptr::null_mut(),
            counter,
            mark_objects: VectorQueue::default(),
            concurrent_marking_in_progress: false,
            mature_sweeping_in_progress: false,
            rc: RefCountHelper::NEW,
        }
    }

    #[inline]
    pub fn new_arc(decs: Arc<Vec<ObjectReference>>, counter: LazySweepingJobsCounter) -> Self {
        Self {
            decs: None,
            decs_arc: Some(decs),
            slice: None,
            new_decs: VectorQueue::default(),
            mmtk: std::ptr::null_mut(),
            counter,
            mark_objects: VectorQueue::default(),
            concurrent_marking_in_progress: false,
            mature_sweeping_in_progress: false,
            rc: RefCountHelper::NEW,
        }
    }

    #[inline]
    pub fn new_array_slice(
        slice: &'static [ObjectReference],
        not_marked: bool,
        counter: LazySweepingJobsCounter,
    ) -> Self {
        Self {
            decs: None,
            decs_arc: None,
            slice: Some((not_marked, slice)),
            new_decs: VectorQueue::default(),
            mmtk: std::ptr::null_mut(),
            counter,
            mark_objects: VectorQueue::default(),
            concurrent_marking_in_progress: false,
            mature_sweeping_in_progress: false,
            rc: RefCountHelper::NEW,
        }
    }

    #[inline(always)]
    pub fn recursive_dec(&mut self, o: ObjectReference) {
        self.new_decs.push(o);
        if self.new_decs.is_full() {
            self.flush()
        }
    }

    #[inline]
    fn new_work(&self, lxr: &LXR<VM>, w: ProcessDecs<VM, COMPRESSED>) {
        if lxr.current_pause().is_none() {
            self.worker()
                .add_work_prioritized(WorkBucketStage::Unconstrained, w);
        } else {
            self.worker().add_work(WorkBucketStage::Unconstrained, w);
        }
    }

    #[inline]
    pub fn flush(&mut self) {
        if !self.new_decs.is_empty() {
            let new_decs = self.new_decs.take();
            let mmtk = unsafe { &*self.mmtk };
            let lxr = mmtk.plan.downcast_ref::<LXR<VM>>().unwrap();
            self.new_work(
                lxr,
                ProcessDecs::new(new_decs, self.counter.clone_with_decs()),
            );
        }
        if !self.mark_objects.is_empty() {
            let objects = self.mark_objects.take();
            let w =
                LXRConcurrentTraceObjects::<_, COMPRESSED>::new(objects, unsafe { &*self.mmtk });
            if crate::args::LAZY_DECREMENTS {
                self.worker().add_work(WorkBucketStage::Unconstrained, w);
            } else {
                self.worker().scheduler().postpone(w);
            }
        }
    }

    #[cold]
    fn process_dead_object(&mut self, o: ObjectReference, immix: &LXR<VM>) {
        crate::stat(|s| {
            s.dead_mature_objects += 1;
            s.dead_mature_volume += o.get_size::<VM>();

            s.dead_mature_rc_objects += 1;
            s.dead_mature_rc_volume += o.get_size::<VM>();

            if !immix.immix_space.in_space(o) {
                s.dead_mature_los_objects += 1;
                s.dead_mature_los_volume += o.get_size::<VM>();

                s.dead_mature_rc_los_objects += 1;
                s.dead_mature_rc_los_volume += o.get_size::<VM>();
            }
        });
        let not_marked = self.concurrent_marking_in_progress && immix.mark(o);
        // println!(" - dead {:?}", o);
        // debug_assert_eq!(self::count(o), 0);
        // Recursively decrease field ref counts
        if false
            && VM::VMScanning::is_obj_array::<COMPRESSED>(o)
            && VM::VMScanning::obj_array_data::<COMPRESSED>(o).bytes() > 1024
        {
            // Buggy. Dead array can be recycled at any time.
            unimplemented!()
        } else if !crate::args().no_recursive_dec {
            o.iterate_fields::<VM, _, COMPRESSED>(
                CLDScanPolicy::Ignore,
                RefScanPolicy::Follow,
                |edge| {
                    let x = edge.load::<COMPRESSED>();
                    if !x.is_null() {
                        // println!(" -- rec dec {:?}.{:?} -> {:?}", o, edge, x);
                        let rc = self.rc.count(x);
                        if rc != MAX_REF_COUNT && rc != 0 {
                            self.recursive_dec(x);
                        }
                        if not_marked && self.concurrent_marking_in_progress && !immix.is_marked(x)
                        {
                            self.mark_objects.push(x);
                        }
                    }
                },
            );
        }
        let in_ix_space = immix.immix_space.in_space(o);
        if !crate::args::HOLE_COUNTING && in_ix_space {
            Block::inc_dead_bytes_sloppy_for_object::<VM>(o);
        }
        if !crate::args::BLOCK_ONLY && in_ix_space {
            self.rc.unmark_straddle_object(o);
        }
        if cfg!(feature = "sanity") || ObjectReference::STRICT_VERIFICATION {
            unsafe { o.to_address::<VM>().store(0xdeadusize) };
        }
        if in_ix_space {
            let block = Block::containing::<VM>(o);
            immix
                .immix_space
                .add_to_possibly_dead_mature_blocks(block, false);
        } else {
            immix.los().rc_free::<COMPRESSED>(o);
        }
    }

    #[inline]
    fn process_decs(
        &mut self,
        decs: &[ObjectReference],
        lxr: &LXR<VM>,
        slice: bool,
        not_marked: bool,
    ) {
        for o in decs {
            // println!("dec {:?}", o);
            if o.is_null() {
                continue;
            }
            if slice {
                if not_marked && self.concurrent_marking_in_progress && !lxr.is_marked(*o) {
                    self.mark_objects.push(*o);
                }
            }
            if self.rc.is_dead_or_stuck(*o)
                || (self.mature_sweeping_in_progress && !lxr.is_marked(*o))
            {
                continue;
            }
            let o =
                if crate::args::RC_MATURE_EVACUATION && object_forwarding::is_forwarded::<VM>(*o) {
                    object_forwarding::read_forwarding_pointer::<VM>(*o)
                } else {
                    *o
                };
            let mut dead = false;
            let _ = self.rc.clone().fetch_update(o, |c| {
                if c == 1 && !dead {
                    dead = true;
                    self.process_dead_object(o, lxr);
                }
                debug_assert!(c <= MAX_REF_COUNT);
                if c == 0 || c == MAX_REF_COUNT {
                    None /* sticky */
                } else {
                    Some(c - 1)
                }
            });
        }
    }
}

impl<VM: VMBinding, const COMPRESSED: bool> GCWork<VM> for ProcessDecs<VM, COMPRESSED> {
    #[inline(always)]
    fn do_work(&mut self, _worker: &mut GCWorker<VM>, mmtk: &'static MMTK<VM>) {
        self.mmtk = mmtk;
        let lxr = mmtk.plan.downcast_ref::<LXR<VM>>().unwrap();
        self.concurrent_marking_in_progress = lxr.concurrent_marking_in_progress();
        self.mature_sweeping_in_progress = lxr.previous_pause() == Some(Pause::FinalMark)
            || lxr.previous_pause() == Some(Pause::FullTraceFast);
        debug_assert!(!crate::plan::barriers::BARRIER_MEASUREMENT);
        if let Some(decs) = std::mem::take(&mut self.decs) {
            self.process_decs(&decs, lxr, false, false);
        } else if let Some(decs) = std::mem::take(&mut self.decs_arc) {
            self.process_decs(&decs, lxr, false, false);
        } else if let Some((not_marked, slice)) = self.slice {
            self.process_decs(slice, lxr, true, not_marked);
        }
        let mut decs = vec![];
        while !self.new_decs.is_empty() {
            decs.clear();
            self.new_decs.swap(&mut decs);
            self.process_decs(&decs, lxr, false, false);
        }
        self.flush();
    }
}

pub struct RCImmixCollectRootEdges<VM: VMBinding> {
    base: ProcessEdgesBase<VM>,
}

impl<VM: VMBinding> ProcessEdgesWork for RCImmixCollectRootEdges<VM> {
    type VM = VM;
    type ScanObjectsWorkType = ScanObjects<Self>;
    const OVERWRITE_REFERENCE: bool = false;
    const RC_ROOTS: bool = true;
    const SCAN_OBJECTS_IMMEDIATELY: bool = true;

    #[inline(always)]
    fn new(edges: Vec<EdgeOf<Self>>, roots: bool, mmtk: &'static MMTK<VM>) -> Self {
        debug_assert!(roots);
        let base = ProcessEdgesBase::new(edges, roots, mmtk);
        Self { base }
    }

    fn trace_object(&mut self, _object: ObjectReference) -> ObjectReference {
        unreachable!()
    }

    #[inline(always)]
    fn process_edges<const COMPRESSED: bool>(&mut self) {
        if !self.edges.is_empty() {
            let roots = std::mem::take(&mut self.edges);
            let mut w = ProcessIncs::<_, EDGE_KIND_ROOT, COMPRESSED>::new(roots);
            GCWork::do_work(&mut w, self.worker(), self.mmtk());
        }
    }

    fn create_scan_work(
        &self,
        _nodes: Vec<ObjectReference>,
        _roots: bool,
    ) -> Self::ScanObjectsWorkType {
        unimplemented!()
    }
}

impl<VM: VMBinding> Deref for RCImmixCollectRootEdges<VM> {
    type Target = ProcessEdgesBase<VM>;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<VM: VMBinding> DerefMut for RCImmixCollectRootEdges<VM> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}