use super::block_allocation::BlockAllocation;
use super::line::*;
use super::{block::*, chunk::ChunkMap, defrag::Defrag};
use crate::plan::immix::Pause;
use crate::plan::ObjectsClosure;
use crate::plan::PlanConstraints;
use crate::policy::space::SpaceOptions;
use crate::policy::space::{CommonSpace, Space, SFT};
use crate::util::heap::layout::heap_layout::{Mmapper, VMMap};
use crate::util::heap::HeapMeta;
use crate::util::heap::PageResource;
use crate::util::heap::VMRequest;
use crate::util::metadata::side_metadata::*;
use crate::util::metadata::{self, compare_exchange_metadata, load_metadata, MetadataSpec};
use crate::util::object_forwarding as ForwardingWord;
use crate::util::{Address, ObjectReference};
use crate::vm::*;
use crate::{
    plan::TransitiveClosure,
    scheduler::{gc_work::ProcessEdgesWork, GCWork, GCWorkScheduler, GCWorker, WorkBucketStage},
    util::{
        heap::FreeListPageResource,
        opaque_pointer::{VMThread, VMWorkerThread},
    },
    AllocationSemantics, CopyContext, MMTK,
};
use atomic::Ordering;
use crossbeam_queue::SegQueue;
use spin::Mutex;
use std::collections::HashSet;
use std::sync::atomic::AtomicUsize;
use std::{
    iter::Step,
    ops::Range,
    sync::{atomic::AtomicU8, Arc},
};

pub static RELEASED_NURSERY_BLOCKS: AtomicUsize = AtomicUsize::new(0);
pub static RELEASED_BLOCKS: AtomicUsize = AtomicUsize::new(0);

pub struct ImmixSpace<VM: VMBinding> {
    common: CommonSpace<VM>,
    pub(super) pr: FreeListPageResource<VM>,
    /// Allocation status for all chunks in immix space
    pub chunk_map: ChunkMap,
    /// Current line mark state
    pub line_mark_state: AtomicU8,
    /// Line mark state in previous GC
    line_unavail_state: AtomicU8,
    /// A list of all reusable blocks
    pub reusable_blocks: BlockList,
    /// Defrag utilities
    pub(super) defrag: Defrag,
    /// Object mark state
    mark_state: u8,
    /// Work packet scheduler
    scheduler: Arc<GCWorkScheduler<VM>>,
    pub block_allocation: BlockAllocation<VM>,
    pub possibly_dead_mature_blocks: Mutex<HashSet<Block>>,
    initial_mark_pause: bool,
    pub pending_release: SegQueue<Block>,
}

unsafe impl<VM: VMBinding> Sync for ImmixSpace<VM> {}

impl<VM: VMBinding> SFT for ImmixSpace<VM> {
    fn name(&self) -> &str {
        self.get_name()
    }
    fn is_live(&self, object: ObjectReference) -> bool {
        if super::REF_COUNT {
            return crate::util::rc::count(object) > 0;
        }
        if self.initial_mark_pause {
            return true;
        }
        self.is_marked(object) || ForwardingWord::is_forwarded::<VM>(object)
    }
    fn is_movable(&self) -> bool {
        super::DEFRAG
    }
    #[cfg(feature = "sanity")]
    fn is_sane(&self) -> bool {
        true
    }
    fn initialize_object_metadata(&self, _object: ObjectReference, _bytes: usize, _alloc: bool) {
        #[cfg(feature = "global_alloc_bit")]
        crate::util::alloc_bit::set_alloc_bit(_object);
    }
}

impl<VM: VMBinding> Space<VM> for ImmixSpace<VM> {
    fn as_space(&self) -> &dyn Space<VM> {
        self
    }
    fn as_sft(&self) -> &(dyn SFT + Sync + 'static) {
        self
    }
    fn get_page_resource(&self) -> &dyn PageResource<VM> {
        &self.pr
    }
    fn common(&self) -> &CommonSpace<VM> {
        &self.common
    }
    fn init(&mut self, _vm_map: &'static VMMap) {
        super::validate_features();
        self.common().init(self.as_space());
        self.block_allocation
            .init(unsafe { &*(self as *const Self) })
    }
    fn release_multiple_pages(&mut self, _start: Address) {
        panic!("immixspace only releases pages enmasse")
    }
}

impl<VM: VMBinding> ImmixSpace<VM> {
    const UNMARKED_STATE: u8 = 0;
    const MARKED_STATE: u8 = 1;

    /// Get side metadata specs
    fn side_metadata_specs() -> Vec<SideMetadataSpec> {
        let mut x = metadata::extract_side_metadata(&if super::BLOCK_ONLY {
            vec![
                MetadataSpec::OnSide(Block::DEFRAG_STATE_TABLE),
                MetadataSpec::OnSide(Block::MARK_TABLE),
                MetadataSpec::OnSide(ChunkMap::ALLOC_TABLE),
                *VM::VMObjectModel::LOCAL_MARK_BIT_SPEC,
            ]
        } else {
            vec![
                MetadataSpec::OnSide(Line::MARK_TABLE),
                MetadataSpec::OnSide(Block::DEFRAG_STATE_TABLE),
                MetadataSpec::OnSide(Block::MARK_TABLE),
                MetadataSpec::OnSide(ChunkMap::ALLOC_TABLE),
                *VM::VMObjectModel::LOCAL_MARK_BIT_SPEC,
            ]
        });
        if crate::plan::immix::REF_COUNT {
            x.push(crate::util::rc::RC_TABLE);
        }
        x
    }

    pub fn new(
        name: &'static str,
        vm_map: &'static VMMap,
        mmapper: &'static Mmapper,
        heap: &mut HeapMeta,
        scheduler: Arc<GCWorkScheduler<VM>>,
        global_side_metadata_specs: Vec<SideMetadataSpec>,
        constraints: &'static PlanConstraints,
    ) -> Self {
        let common = CommonSpace::new(
            SpaceOptions {
                name,
                movable: true,
                immortal: false,
                zeroed: true,
                vmrequest: VMRequest::discontiguous(),
                side_metadata_specs: SideMetadataContext {
                    global: global_side_metadata_specs,
                    local: Self::side_metadata_specs(),
                },
                needs_log_bit: constraints.needs_log_bit,
                needs_field_log_bit: constraints.needs_field_log_bit,
            },
            vm_map,
            mmapper,
            heap,
        );
        ImmixSpace {
            pr: if common.vmrequest.is_discontiguous() {
                FreeListPageResource::new_discontiguous(0, vm_map)
            } else {
                FreeListPageResource::new_contiguous(common.start, common.extent, 0, vm_map)
            },
            common,
            chunk_map: ChunkMap::new(),
            line_mark_state: AtomicU8::new(Line::RESET_MARK_STATE),
            line_unavail_state: AtomicU8::new(Line::RESET_MARK_STATE),
            reusable_blocks: BlockList::default(),
            defrag: Defrag::default(),
            mark_state: Self::UNMARKED_STATE,
            scheduler,
            block_allocation: BlockAllocation::new(),
            possibly_dead_mature_blocks: Default::default(),
            initial_mark_pause: false,
            pending_release: Default::default(),
        }
    }

    /// Get the number of defrag headroom pages.
    pub fn defrag_headroom_pages(&self) -> usize {
        self.defrag.defrag_headroom_pages(self)
    }

    /// Check if current GC is a defrag GC.
    #[inline(always)]
    pub fn in_defrag(&self) -> bool {
        self.defrag.in_defrag()
    }

    /// check if the current GC should do defragmentation.
    pub fn decide_whether_to_defrag(
        &self,
        emergency_collection: bool,
        collect_whole_heap: bool,
        collection_attempts: usize,
        user_triggered_collection: bool,
        full_heap_system_gc: bool,
    ) -> bool {
        self.defrag.decide_whether_to_defrag(
            emergency_collection,
            collect_whole_heap,
            collection_attempts,
            user_triggered_collection,
            self.reusable_blocks.len() == 0,
            full_heap_system_gc,
        );
        self.defrag.in_defrag()
    }

    /// Get work packet scheduler
    fn scheduler(&self) -> &GCWorkScheduler<VM> {
        &self.scheduler
    }

    pub fn prepare_rc(&mut self, pause: Pause) {
        debug_assert_ne!(pause, Pause::FullTraceDefrag);
        let num_workers = self.scheduler().worker_group().worker_count();
        let work_packets = if crate::flags::LOCK_FREE_BLOCK_ALLOCATION {
            self.block_allocation
                .reset_and_generate_nursery_sweep_tasks(num_workers)
        } else {
            unreachable!();
        };
        self.scheduler().work_buckets[WorkBucketStage::RCReleaseNursery].bulk_add(work_packets);
        if pause == Pause::FullTraceFast || pause == Pause::InitialMark {
            // Update mark_state
            if VM::VMObjectModel::LOCAL_MARK_BIT_SPEC.is_on_side() {
                self.mark_state = Self::MARKED_STATE;
            } else {
                // For header metadata, we use cyclic mark bits.
                unimplemented!("cyclic mark bits is not supported at the moment");
            }
            let space = unsafe { &mut *(self as *mut Self) };
            let work_packets = self.chunk_map.generate_prepare_tasks::<VM>(space, None);
            self.scheduler().work_buckets[WorkBucketStage::Prepare].bulk_add(work_packets);
        }
    }

    pub fn release_rc(&mut self, pause: Pause) {
        debug_assert_ne!(pause, Pause::FullTraceDefrag);
        self.block_allocation.reset();
        if pause == Pause::FullTraceFast || pause == Pause::FinalMark {
            let work_packets = self.chunk_map.generate_dead_cycle_sweep_tasks();
            if crate::flags::LAZY_DECREMENTS {
                self.scheduler().postpone_all(work_packets);
            } else {
                self.scheduler().work_buckets[WorkBucketStage::RCFullHeapRelease]
                    .bulk_add(work_packets);
            }
            while let Some(x) = self.pending_release.pop() {
                self.release_block(x, false);
            }
        }
    }

    pub fn prepare(&mut self, initial_mark_pause: bool) {
        self.initial_mark_pause = initial_mark_pause;
        debug_assert!(!crate::flags::REF_COUNT);
        self.block_allocation.reset();
        // Update mark_state
        if VM::VMObjectModel::LOCAL_MARK_BIT_SPEC.is_on_side() {
            self.mark_state = Self::MARKED_STATE;
        } else {
            // For header metadata, we use cyclic mark bits.
            unimplemented!("cyclic mark bits is not supported at the moment");
        }
        // Prepare defrag info
        if super::DEFRAG {
            self.defrag.prepare(self);
        }
        // Prepare each block for GC
        let threshold = self.defrag.defrag_spill_threshold.load(Ordering::Acquire);
        // # Safety: ImmixSpace reference is always valid within this collection cycle.
        let space = unsafe { &*(self as *const Self) };
        let work_packets = self.chunk_map.generate_prepare_tasks::<VM>(
            space,
            if space.in_defrag() {
                Some(threshold)
            } else {
                None
            },
        );
        self.scheduler().work_buckets[WorkBucketStage::Prepare].bulk_add(work_packets);
        // Update line mark state
        if !super::BLOCK_ONLY {
            self.line_mark_state.fetch_add(1, Ordering::AcqRel);
            if self.line_mark_state.load(Ordering::Acquire) > Line::MAX_MARK_STATE {
                self.line_mark_state
                    .store(Line::RESET_MARK_STATE, Ordering::Release);
            }
        }
    }

    pub fn release(&mut self) {
        debug_assert!(!crate::flags::REF_COUNT);
        self.block_allocation.reset();
        // Update line_unavail_state for hole searching afte this GC.
        if !super::BLOCK_ONLY {
            self.line_unavail_state.store(
                self.line_mark_state.load(Ordering::Acquire),
                Ordering::Release,
            );
        }
        // Clear reusable blocks list
        if !super::BLOCK_ONLY {
            self.reusable_blocks.reset();
        }
        // Sweep chunks and blocks
        // # Safety: ImmixSpace reference is always valid within this collection cycle.
        let space = unsafe { &*(self as *const Self) };
        let work_packets = self.chunk_map.generate_sweep_tasks(space, false);
        self.scheduler().work_buckets[WorkBucketStage::Release].bulk_add(work_packets);
        if super::DEFRAG {
            self.defrag.release(self)
        }
        self.initial_mark_pause = false;
    }

    /// Release a block.
    pub fn release_block(&self, block: Block, nursery: bool) {
        if crate::flags::LOG_PER_GC_STATE {
            if nursery {
                RELEASED_NURSERY_BLOCKS.fetch_add(1, Ordering::SeqCst);
            }
            RELEASED_BLOCKS.fetch_add(1, Ordering::SeqCst);
        }
        if self.common().needs_log_bit {
            if crate::flags::BARRIER_MEASUREMENT || self.common().needs_log_bit {
                block.clear_log_table::<VM>();
            }
        }
        block.deinit();
        self.pr.release_pages(block.start());
    }

    /// Allocate a clean block.
    #[inline(always)]
    pub fn get_clean_block(&self, tls: VMThread, copy: bool) -> Option<Block> {
        self.block_allocation.get_clean_block(tls, copy)
    }

    /// Pop a reusable block from the reusable block list.
    #[inline(always)]
    pub fn get_reusable_block(&self, copy: bool) -> Option<Block> {
        self.block_allocation.get_reusable_block(copy)
    }

    /// Trace and mark objects without evacuation.
    #[inline(always)]
    pub fn fast_trace_object(
        &self,
        trace: &mut impl TransitiveClosure,
        object: ObjectReference,
    ) -> ObjectReference {
        self.trace_object_without_moving(trace, object)
    }

    /// Trace and mark objects. If the current object is in defrag block, then do evacuation as well.
    #[inline(always)]
    pub fn trace_object(
        &self,
        trace: &mut impl TransitiveClosure,
        object: ObjectReference,
        semantics: AllocationSemantics,
        copy_context: &mut impl CopyContext,
    ) -> ObjectReference {
        #[cfg(feature = "global_alloc_bit")]
        debug_assert!(
            crate::util::alloc_bit::is_alloced(object),
            "{:x}: alloc bit not set",
            object
        );
        if Block::containing::<VM>(object).is_defrag_source() {
            self.trace_object_with_opportunistic_copy(trace, object, semantics, copy_context)
        } else {
            self.trace_object_without_moving(trace, object)
        }
    }

    /// Trace and mark objects without evacuation.
    #[inline(always)]
    pub fn trace_object_without_moving(
        &self,
        trace: &mut impl TransitiveClosure,
        object: ObjectReference,
    ) -> ObjectReference {
        if self.attempt_mark(object) {
            // Mark block and lines
            if !super::BLOCK_ONLY {
                if !super::MARK_LINE_AT_SCAN_TIME {
                    self.mark_lines(object);
                }
            } else {
                let block = Block::containing::<VM>(object);
                let state = block.get_state();
                if state != BlockState::Nursery && state != BlockState::Marked {
                    block.set_state(BlockState::Marked);
                }
            }
            // Visit node
            trace.process_node(object);
        }
        object
    }

    /// Trace object and do evacuation if required.
    #[allow(clippy::assertions_on_constants)]
    #[inline(always)]
    pub fn trace_object_with_opportunistic_copy(
        &self,
        trace: &mut impl TransitiveClosure,
        object: ObjectReference,
        semantics: AllocationSemantics,
        copy_context: &mut impl CopyContext,
    ) -> ObjectReference {
        debug_assert!(!super::BLOCK_ONLY);
        let forwarding_status = ForwardingWord::attempt_to_forward::<VM>(object);
        if ForwardingWord::state_is_forwarded_or_being_forwarded(forwarding_status) {
            ForwardingWord::spin_and_get_forwarded_object::<VM>(object, forwarding_status)
        } else if self.is_marked(object) {
            ForwardingWord::clear_forwarding_bits::<VM>(object);
            object
        } else {
            let new_object = if Self::is_pinned(object) || self.defrag.space_exhausted() {
                self.attempt_mark(object);
                ForwardingWord::clear_forwarding_bits::<VM>(object);
                Block::containing::<VM>(object).set_state(BlockState::Marked);
                object
            } else {
                #[cfg(feature = "global_alloc_bit")]
                crate::util::alloc_bit::unset_alloc_bit(object);
                ForwardingWord::forward_object::<VM, _>(object, semantics, copy_context)
            };
            if !super::MARK_LINE_AT_SCAN_TIME {
                self.mark_lines(new_object);
            }
            debug_assert!({
                let state = Block::containing::<VM>(new_object).get_state();
                state == BlockState::Marked || state == BlockState::Nursery
            });
            trace.process_node(new_object);
            new_object
        }
    }

    /// Mark all the lines that the given object spans.
    #[allow(clippy::assertions_on_constants)]
    #[inline]
    pub fn mark_lines(&self, object: ObjectReference) {
        debug_assert!(!super::BLOCK_ONLY);
        Line::mark_lines_for_object::<VM>(object, self.line_mark_state.load(Ordering::Acquire));
    }

    /// Atomically mark an object.
    #[inline(always)]
    pub fn attempt_mark(&self, object: ObjectReference) -> bool {
        loop {
            let old_value = load_metadata::<VM>(
                &VM::VMObjectModel::LOCAL_MARK_BIT_SPEC,
                object,
                None,
                Some(Ordering::SeqCst),
            ) as u8;
            if old_value == self.mark_state {
                return false;
            }

            if compare_exchange_metadata::<VM>(
                &VM::VMObjectModel::LOCAL_MARK_BIT_SPEC,
                object,
                old_value as usize,
                self.mark_state as usize,
                None,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                break;
            }
        }
        true
    }

    /// Check if an object is marked.
    #[inline(always)]
    pub fn is_marked(&self, object: ObjectReference) -> bool {
        if crate::flags::REF_COUNT && crate::flags::CONCURRENT_MARKING {
            // debug_assert!(!crate::flags::RC_EVACUATE_NURSERY);
            // Treat young objects as marked.
            // FIXME: What about nursery object in reusable lines???
            if Block::containing::<VM>(object).get_state() == BlockState::Nursery {
                return true;
            }
        }
        if !crate::flags::REF_COUNT && crate::flags::CONCURRENT_MARKING {
            // Treat young objects as marked.
            if Block::containing::<VM>(object).get_state() == BlockState::Nursery {
                return true;
            }
        }
        let old_value = load_metadata::<VM>(
            &VM::VMObjectModel::LOCAL_MARK_BIT_SPEC,
            object,
            None,
            Some(Ordering::SeqCst),
        ) as u8;
        old_value == self.mark_state
    }

    #[inline(always)]
    pub fn mark_bit(&self, object: ObjectReference) -> bool {
        let old_value = load_metadata::<VM>(
            &VM::VMObjectModel::LOCAL_MARK_BIT_SPEC,
            object,
            None,
            Some(Ordering::SeqCst),
        ) as u8;
        old_value == self.mark_state
    }

    /// Check if an object is pinned.
    #[inline(always)]
    fn is_pinned(_object: ObjectReference) -> bool {
        // TODO(wenyuzhao): Object pinning not supported yet.
        false
    }

    /// Hole searching.
    ///
    /// Linearly scan lines in a block to search for the next
    /// hole, starting from the given line.
    ///
    /// Returns None if the search could not find any more holes.
    #[allow(clippy::assertions_on_constants)]
    pub fn get_next_available_lines(&self, search_start: Line) -> Option<Range<Line>> {
        debug_assert!(!super::BLOCK_ONLY);
        if super::REF_COUNT {
            self.rc_get_next_available_lines(search_start)
        } else {
            self.normal_get_next_available_lines(search_start)
        }
    }

    /// Search holes by ref-counts instead of line marks
    #[allow(clippy::assertions_on_constants)]
    #[inline]
    pub fn rc_get_next_available_lines(&self, search_start: Line) -> Option<Range<Line>> {
        debug_assert!(!super::BLOCK_ONLY);
        debug_assert!(super::REF_COUNT);
        let block = search_start.block();
        let mut cursor = search_start;
        let limit = block.lines().end;
        // Find start
        while cursor < limit {
            if cursor.rc_dead() {
                break;
            }
            cursor = Line::forward(cursor, 1);
        }
        // Conservatively skip the next line
        cursor = Line::forward(cursor, 1);
        if cursor >= limit {
            return None;
        }
        let start = cursor;
        // Find limit
        while cursor < limit {
            if !cursor.rc_dead() {
                break;
            }
            // if crate::plan::immix::CONCURRENT_MARKING {
            //     mark_data.set(cursor, current_state);
            // }
            cursor = Line::forward(cursor, 1);
        }
        if cursor == start {
            return None;
        }
        let end = cursor;
        if self.common.needs_log_bit {
            Line::clear_log_table::<VM>(start..end);
        }
        Some(start..end)
    }

    #[allow(clippy::assertions_on_constants)]
    #[inline]
    pub fn normal_get_next_available_lines(&self, search_start: Line) -> Option<Range<Line>> {
        debug_assert!(!super::BLOCK_ONLY);
        debug_assert!(!super::REF_COUNT);
        let unavail_state = self.line_unavail_state.load(Ordering::Acquire);
        let current_state = self.line_mark_state.load(Ordering::Acquire);
        let block = search_start.block();
        let mut mark_data = block.line_mark_table();
        let start_cursor = search_start.get_index_within_block();
        let mut cursor = start_cursor;
        // Find start
        while cursor < mark_data.len() {
            let mark = mark_data.get(cursor);
            if mark != unavail_state && mark != current_state {
                break;
            }
            cursor += 1;
        }
        if cursor == mark_data.len() {
            return None;
        }
        let start = Line::forward(search_start, cursor - start_cursor);
        // Find limit
        while cursor < mark_data.len() {
            let mark = mark_data.get(cursor);
            if mark == unavail_state || mark == current_state {
                break;
            }
            if crate::plan::immix::CONCURRENT_MARKING {
                mark_data.set(cursor, current_state);
            }
            cursor += 1;
        }
        let end = Line::forward(search_start, cursor - start_cursor);
        if self.common.needs_log_bit {
            Line::clear_log_table::<VM>(start..end);
        }
        Some(start..end)
    }
}

/// A work packet to scan the fields of each objects and mark lines.
pub struct ScanObjectsAndMarkLines<Edges: ProcessEdgesWork> {
    buffer: Vec<ObjectReference>,
    concurrent: bool,
    immix_space: &'static ImmixSpace<Edges::VM>,
}

impl<Edges: ProcessEdgesWork> ScanObjectsAndMarkLines<Edges> {
    pub fn new(
        buffer: Vec<ObjectReference>,
        concurrent: bool,
        immix_space: &'static ImmixSpace<Edges::VM>,
    ) -> Self {
        if concurrent {
            crate::NUM_CONCURRENT_TRACING_PACKETS.fetch_add(1, Ordering::SeqCst);
        }
        Self {
            buffer,
            concurrent,
            immix_space,
        }
    }
}

impl<E: ProcessEdgesWork> GCWork<E::VM> for ScanObjectsAndMarkLines<E> {
    fn do_work(&mut self, worker: &mut GCWorker<E::VM>, mmtk: &'static MMTK<E::VM>) {
        trace!("ScanObjectsAndMarkLines");
        let mut closure = ObjectsClosure::<E>::new(
            mmtk,
            vec![],
            worker,
            if self.concurrent {
                WorkBucketStage::Unconstrained
            } else {
                WorkBucketStage::Closure
            },
        );
        for object in &self.buffer {
            <E::VM as VMBinding>::VMScanning::scan_object(
                &mut closure,
                *object,
                VMWorkerThread(VMThread::UNINITIALIZED),
            );
            if super::MARK_LINE_AT_SCAN_TIME
                && !super::BLOCK_ONLY
                && self.immix_space.in_space(*object)
            {
                self.immix_space.mark_lines(*object);
            }
        }
    }
}

impl<E: ProcessEdgesWork> Drop for ScanObjectsAndMarkLines<E> {
    fn drop(&mut self) {
        if self.concurrent {
            crate::NUM_CONCURRENT_TRACING_PACKETS.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

pub struct ReleaseRCNursery<VM: VMBinding> {
    space: &'static mut ImmixSpace<VM>,
}

impl<VM: VMBinding> GCWork<VM> for ReleaseRCNursery<VM> {
    fn do_work(&mut self, worker: &mut GCWorker<VM>, _mmtk: &'static MMTK<VM>) {
        let num_workers = worker.scheduler().worker_group().worker_count();
        let work_packets = if crate::flags::LOCK_FREE_BLOCK_ALLOCATION {
            self.space
                .block_allocation
                .reset_and_generate_nursery_sweep_tasks(num_workers)
        } else {
            let space: &'static ImmixSpace<VM> = unsafe { &*(self.space as *const ImmixSpace<VM>) };
            self.space.chunk_map.generate_sweep_tasks::<VM>(space, true)
        };
        worker.scheduler().work_buckets[WorkBucketStage::RCReleaseNursery].bulk_add(work_packets);
    }
}
