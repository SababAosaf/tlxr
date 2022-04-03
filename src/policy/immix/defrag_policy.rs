use super::cset::RegionSelection;
use super::{block::Block, cset::CollectionSet, region::Region};
use crate::policy::immix::line::Line;
use crate::vm::ActivePlan;
use crate::{
    plan::immix::{Immix, Pause},
    vm::VMBinding,
    MMTK,
};
use atomic::Ordering;
use crossbeam_queue::SegQueue;
use downcast_rs::Downcast;
use std::{ops::ControlFlow, sync::atomic::AtomicUsize};

pub fn create_defrag_policy<VM: VMBinding>() -> Box<dyn DefragPolicy<VM>> {
    let policy = crate::args::LXR_DEFRAG_POLICY.as_ref().map(|x| x.as_str());
    println!("{}", policy.unwrap_or("DefaultDefragPolicy"));
    match policy {
        Some("NoDefrag") => {
            println!("NoDefragPolicy");
            box NoDefragPolicy
        }
        Some("SimpleIncrementalDefrag") => {
            println!("SimpleIncrementalDefragPolicy");
            box SimpleIncrementalDefragPolicy
        }
        Some("SimpleIncrementalDefrag2") => {
            println!("SimpleIncrementalDefragPolicy2");
            box SimpleIncrementalDefragPolicy2::default()
        }
        Some("SimpleIncrementalDefrag3") => {
            println!("SimpleIncrementalDefragPolicy3");
            box SimpleIncrementalDefragPolicy3::default()
        }
        _ if crate::args::LXR_EAGER_DEFRAG_SELECTION => {
            println!("EagerDefragPolicy");
            box EagerDefragPolicy::default()
        }
        _ => {
            println!("DefaultDefragPolicy");
            box DefaultDefragPolicy::default()
        }
    }
}

pub trait DefragPolicy<VM: VMBinding>: Downcast {
    /// Called before evacuation to select the collection set.
    fn select(&'static self, mmtk: &'static MMTK<VM>);
    /// For each iteration of evacuation, choose a subset of regions from the collection set to evacuate.
    fn schedule(&self, cset: &CollectionSet, regions: &mut Vec<Region>) -> RegionSelection;
    /// Called when start evacuation the region.
    fn notify_defrag_start(&self, _region: Region) {}
    /// Called when the evacuation of the region is finished.
    fn notify_defrag_end(&self, _region: Region) {}
    /// Called when the evacuation is paused
    fn notify_evacuation_stop(&self) {}
}

impl_downcast!(DefragPolicy<VM> where VM: VMBinding);

struct NoDefragPolicy;

impl<VM: VMBinding> DefragPolicy<VM> for NoDefragPolicy {
    fn select(&self, mmtk: &'static MMTK<VM>) {
        let immix_space = &mmtk.plan.downcast_ref::<Immix<VM>>().unwrap().immix_space;
        immix_space.collection_set.set_reigons(vec![]);
    }
    fn schedule(&self, _cset: &CollectionSet, _regions: &mut Vec<Region>) -> RegionSelection {
        unreachable!()
    }
}

struct SimpleIncrementalDefragPolicy;

impl SimpleIncrementalDefragPolicy {
    fn select_with_sorting<VM: VMBinding>(&self, mmtk: &'static MMTK<VM>) {
        let immix_space = &mmtk.plan.downcast_ref::<Immix<VM>>().unwrap().immix_space;
        let n = crate::args::LXR_DEFRAG_N.unwrap() * *crate::args::LXR_DEFRAG_M;
        let threshold = crate::args::LXR_DEFRAG_BLOCK_LIVENESS_THRESHOLD.unwrap();
        let mut regions = vec![];
        for chunk in immix_space.chunk_map.committed_chunks() {
            for region in chunk.regions() {
                let mut live_blocks = 0usize;
                let mut target_blocks = 0usize;
                for block in region.committed_mature_blocks() {
                    live_blocks += 1;
                    if block.live_bytes() * 100 <= threshold * Block::BYTES {
                        target_blocks += 1;
                    }
                }
                if live_blocks > 0 {
                    regions.push((region, target_blocks));
                }
            }
        }
        regions.sort_by_key(|x| x.1);
        regions.reverse();
        let regions = regions.iter().take(n).map(|x| x.0).collect::<Vec<_>>();
        for region in &regions {
            region.set_defrag_source();
            for block in region.committed_mature_blocks() {
                block.set_as_defrag_source(true);
            }
        }
        immix_space.collection_set.set_reigons(regions);
    }
}

impl<VM: VMBinding> DefragPolicy<VM> for SimpleIncrementalDefragPolicy {
    fn select(&self, mmtk: &'static MMTK<VM>) {
        assert_eq!(*crate::args::LXR_DEFRAG_COALESCE_M, 1);
        assert!(!crate::args::LXR_EAGER_DEFRAG_SELECTION);
        if *crate::args::LXR_SIMPLE_INCREMENTAL_DEFRAG_SORT_REGIONS {
            return self.select_with_sorting(mmtk);
        }
        let immix_space = &mmtk.plan.downcast_ref::<Immix<VM>>().unwrap().immix_space;
        let n = crate::args::LXR_DEFRAG_N.unwrap() * *crate::args::LXR_DEFRAG_M;
        let mut regions = vec![];
        immix_space.walk_regions_in_address_order(|region| {
            region.set_defrag_source();
            regions.push(region);
            for block in region.committed_mature_blocks() {
                block.set_as_defrag_source(true);
            }
            if regions.len() >= n {
                ControlFlow::BREAK
            } else {
                ControlFlow::CONTINUE
            }
        });
        immix_space.collection_set.set_reigons(regions);
    }

    fn schedule(&self, cset: &CollectionSet, regions: &mut Vec<Region>) -> RegionSelection {
        if cset.retired_regions() >= *crate::args::LXR_DEFRAG_M {
            Ok(vec![])
        } else {
            CollectionSet::pop_prioritized_regions(regions)
        }
    }
}

#[derive(Default)]
struct SimpleIncrementalDefragPolicy2 {
    processed_blocks: AtomicUsize,
    per_pause_budget: AtomicUsize,
}

impl<VM: VMBinding> DefragPolicy<VM> for SimpleIncrementalDefragPolicy2 {
    fn select(&self, mmtk: &'static MMTK<VM>) {
        assert_eq!(*crate::args::LXR_DEFRAG_COALESCE_M, 1);
        assert!(!crate::args::LXR_EAGER_DEFRAG_SELECTION);
        let immix_space = &mmtk.plan.downcast_ref::<Immix<VM>>().unwrap().immix_space;
        let regions = crate::args::LXR_DEFRAG_N.unwrap() * *crate::args::LXR_DEFRAG_M;
        let n = regions * Region::BLOCKS;
        let mut regions = vec![];
        let mut blocks = 0usize;
        let threshold = crate::args::LXR_DEFRAG_BLOCK_LIVENESS_THRESHOLD.unwrap();
        immix_space.walk_regions_in_address_order(|region| {
            region.set_defrag_source();
            regions.push(region);
            let mut c = 0usize;
            for block in region.committed_mature_blocks() {
                if block.live_bytes() * 100 <= threshold * Block::BYTES {
                    block.set_as_defrag_source(true);
                    c += 1;
                }
            }
            blocks += c;
            if crate::args::LOG_PER_GC_STATE {
                println!(
                    " - Defrag {:?} defrag {} (total {} / {}) blocks",
                    region, c, blocks, n
                );
            }
            if blocks >= n {
                ControlFlow::BREAK
            } else {
                ControlFlow::CONTINUE
            }
        });
        self.per_pause_budget.store(
            Region::BLOCKS * *crate::args::LXR_DEFRAG_M,
            Ordering::SeqCst,
        );
        immix_space.collection_set.set_reigons(regions);
    }
    fn schedule(&self, _cset: &CollectionSet, regions: &mut Vec<Region>) -> RegionSelection {
        if self.processed_blocks.load(Ordering::Relaxed)
            >= self.per_pause_budget.load(Ordering::Relaxed)
        {
            Ok(vec![])
        } else {
            CollectionSet::pop_prioritized_regions(regions)
        }
    }
    fn notify_defrag_end(&self, region: Region) {
        let mut blocks = 0usize;
        for block in region.committed_blocks() {
            if block.is_defrag_source() {
                blocks += 1;
            }
        }
        self.processed_blocks.fetch_add(blocks, Ordering::Relaxed);
    }
    fn notify_evacuation_stop(&self) {
        self.processed_blocks.store(0, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct SimpleIncrementalDefragPolicy3 {
    fragmented_regions: SegQueue<Vec<(Region, usize)>>,
    fragmented_regions_size: AtomicUsize,
}

impl SimpleIncrementalDefragPolicy3 {
    pub fn select_mature_evacuation_candidates<VM: VMBinding>(
        &self,
        _pause: Pause,
        _total_pages: usize,
        immix: &Immix<VM>,
    ) {
        debug_assert!(crate::args::RC_MATURE_EVACUATION);
        // Sort regions by score
        let mut regions = Vec::with_capacity(self.fragmented_regions_size.load(Ordering::SeqCst));
        while let Some(mut x) = self.fragmented_regions.pop() {
            regions.append(&mut x);
        }
        regions.sort_by_key(|x| x.1);
        // Select blocks up to space limit
        let mut cset = vec![];
        let threshold =
            crate::args::LXR_DEFRAG_BLOCK_LIVENESS_THRESHOLD.unwrap() * Block::BYTES / 100;
        let n = crate::args::LXR_DEFRAG_N.unwrap()
            * *crate::args::LXR_DEFRAG_M
            * *crate::args::LXR_DEFRAG_COALESCE_M;
        while let Some((region, _)) = regions.pop() {
            region.set_defrag_source();
            cset.push(region);
            if crate::args::LOG_PER_GC_STATE {
                println!(" - Defrag {:?}", region);
            }
            for block in region
                .committed_mature_blocks()
                .filter(|b| b.live_bytes() <= threshold)
            {
                block.set_as_defrag_source(true)
            }
            if cset.len() >= n {
                break;
            }
        }
        immix.immix_space.collection_set.set_reigons(cset);
    }
}

impl<VM: VMBinding> DefragPolicy<VM> for SimpleIncrementalDefragPolicy3 {
    fn select(&'static self, mmtk: &'static MMTK<VM>) {
        assert!(!crate::args::LXR_EAGER_DEFRAG_SELECTION);
        let threshold =
            crate::args::LXR_DEFRAG_BLOCK_LIVENESS_THRESHOLD.unwrap() * Block::BYTES / 100;
        let immix = mmtk.plan.downcast_ref::<Immix<VM>>().unwrap();
        let space = &immix.immix_space;
        let policy = self;
        self.fragmented_regions_size.store(0, Ordering::SeqCst);
        space.run_per_chunk_tasks(
            "CollectRegions",
            move |chunk| {
                let mut regions = vec![];
                // Calculate score
                for region in chunk.regions() {
                    let mut total_live_bytes = 0usize;
                    let mut total_reclaimable_bytes = 0usize;
                    for block in region.committed_mature_blocks() {
                        let live_bytes = block.live_bytes();
                        total_live_bytes += live_bytes;
                        if live_bytes <= threshold {
                            total_reclaimable_bytes += Block::BYTES;
                        }
                    }
                    if total_reclaimable_bytes > 0 && total_live_bytes > 0 {
                        let score = total_reclaimable_bytes * 1000 / total_live_bytes;
                        regions.push((region, score));
                    }
                }
                policy
                    .fragmented_regions_size
                    .fetch_add(regions.len(), Ordering::SeqCst);
                policy.fragmented_regions.push(regions);
            },
            move || {
                policy.select_mature_evacuation_candidates(
                    immix.current_pause().unwrap(),
                    mmtk.plan.get_total_pages(),
                    &immix,
                )
            },
        );
    }
    fn schedule(&self, cset: &CollectionSet, regions: &mut Vec<Region>) -> RegionSelection {
        if cset.retired_regions()
            >= *crate::args::LXR_DEFRAG_M * *crate::args::LXR_DEFRAG_COALESCE_M
        {
            Ok(vec![])
        } else {
            CollectionSet::pop_prioritized_regions(regions)
        }
    }
}

#[derive(Default)]
struct DefaultDefragPolicy {
    fragmented_regions: SegQueue<Vec<(Region, usize)>>,
    fragmented_regions_size: AtomicUsize,
}

impl DefaultDefragPolicy {
    pub fn select_mature_evacuation_candidates<VM: VMBinding>(
        &self,
        _pause: Pause,
        _total_pages: usize,
        immix: &Immix<VM>,
    ) {
        debug_assert!(crate::args::RC_MATURE_EVACUATION);
        let available_clean_pages_for_defrag = VM::VMActivePlan::global().get_total_pages()
            + immix.immix_space.defrag_headroom_pages()
            - VM::VMActivePlan::global().get_pages_reserved();
        let available_clean_bytes_for_defrag = available_clean_pages_for_defrag << 12;
        debug_assert!(crate::args::RC_MATURE_EVACUATION);
        // Sort regions by score
        let mut regions = Vec::with_capacity(self.fragmented_regions_size.load(Ordering::SeqCst));
        while let Some(mut x) = self.fragmented_regions.pop() {
            regions.append(&mut x);
        }
        regions.sort_by_key(|x| x.1);
        // Select blocks up to space limit
        let mut cset = vec![];
        let threshold =
            crate::args::LXR_DEFRAG_BLOCK_LIVENESS_THRESHOLD.unwrap() * Block::BYTES / 100;
        let mut copy_bytes = 0usize;
        while let Some((region, s)) = regions.pop() {
            region.set_defrag_source();
            cset.push(region);
            let mut blocks = 0;
            for block in region
                .committed_mature_blocks()
                .filter(|b| b.live_bytes() <= threshold)
            {
                block.set_as_defrag_source(true);
                copy_bytes += block.live_bytes();
                blocks += 1;
            }
            if crate::args::LOG_PER_GC_STATE {
                println!(" - Defrag {:?} score={} blocks={}", region, s, blocks);
            }
            if copy_bytes >= available_clean_bytes_for_defrag {
                break;
            }
        }
        cset.reverse();
        immix.immix_space.collection_set.set_reigons(cset);
    }
}

impl<VM: VMBinding> DefragPolicy<VM> for DefaultDefragPolicy {
    fn select(&'static self, mmtk: &'static MMTK<VM>) {
        assert!(!crate::args::LXR_EAGER_DEFRAG_SELECTION);
        let threshold =
            crate::args::LXR_DEFRAG_BLOCK_LIVENESS_THRESHOLD.unwrap() * Block::BYTES / 100;
        let immix = mmtk.plan.downcast_ref::<Immix<VM>>().unwrap();
        let space = &immix.immix_space;
        let policy = self;
        self.fragmented_regions_size.store(0, Ordering::SeqCst);
        space.run_per_chunk_tasks(
            "CollectRegions",
            move |chunk| {
                let mut regions = vec![];
                // Calculate score
                for region in chunk.regions() {
                    let mut total_live_bytes = 0usize;
                    let mut total_reclaimable_bytes = 0usize;
                    for block in region.committed_mature_blocks() {
                        let live_bytes = block.live_bytes();
                        total_live_bytes += live_bytes;
                        if live_bytes <= threshold {
                            total_reclaimable_bytes += 1;
                        }
                    }
                    if total_reclaimable_bytes > 0 && total_live_bytes > 0 {
                        let score = total_reclaimable_bytes
                            * total_reclaimable_bytes
                            * total_reclaimable_bytes
                            * 1000000
                            / total_live_bytes;
                        regions.push((region, score));
                    }
                }
                policy
                    .fragmented_regions_size
                    .fetch_add(regions.len(), Ordering::SeqCst);
                policy.fragmented_regions.push(regions);
            },
            move || {
                policy.select_mature_evacuation_candidates(
                    immix.current_pause().unwrap(),
                    mmtk.plan.get_total_pages(),
                    &immix,
                )
            },
        );
    }

    fn schedule(&self, _cset: &CollectionSet, regions: &mut Vec<Region>) -> RegionSelection {
        CollectionSet::incremental_test(regions)?;
        CollectionSet::time_limit_test()?;
        CollectionSet::pop_prioritized_regions(regions)
    }
}

#[derive(Default)]
struct EagerDefragPolicy {
    fragmented_blocks: SegQueue<Vec<(Block, usize)>>,
    fragmented_blocks_size: AtomicUsize,
}

impl EagerDefragPolicy {
    pub fn select_mature_evacuation_candidates<VM: VMBinding>(
        &self,
        _pause: Pause,
        _total_pages: usize,
        immix: &Immix<VM>,
    ) {
        debug_assert!(crate::args::RC_MATURE_EVACUATION);
        let available_clean_pages_for_defrag = immix.immix_space.defrag_headroom_pages();
        let available_clean_bytes_for_defrag = available_clean_pages_for_defrag << 12;
        debug_assert!(crate::args::RC_MATURE_EVACUATION);
        // Sort blocks by liveness
        let choose_all = *crate::args::LXR_DEFRAG_FORCE_SELECT_ALL_BLOCKS;
        let mut blocks = Vec::with_capacity(self.fragmented_blocks_size.load(Ordering::SeqCst));
        while let Some(mut x) = self.fragmented_blocks.pop() {
            blocks.append(&mut x);
        }
        if !choose_all {
            blocks.sort_by_key(|x| x.1);
            blocks.reverse();
        }
        // Select blocks up to space limit
        let mut cset = vec![];
        let mut copy_bytes = 0usize;
        let rank_by_holes = *crate::args::FRAG_BLOCK_SELECTION;
        while let Some((block, live_bytes)) = blocks.pop() {
            copy_bytes += if rank_by_holes {
                (Block::LINES - block.calc_dead_lines()) << Line::LOG_BYTES
            } else {
                live_bytes
            };
            block.set_as_defrag_source(true);
            let region = block.region();
            if !region.is_defrag_source() {
                region.set_defrag_source();
                cset.push(region);
            }
            if crate::args::LOG_PER_GC_STATE {
                println!(
                    " - Defrag {:?} live={} holes={} total={}M",
                    block,
                    (Block::LINES - block.calc_dead_lines()) << Line::LOG_BYTES,
                    block.calc_holes(),
                    copy_bytes >> 20
                );
            }
            if !choose_all && copy_bytes >= available_clean_bytes_for_defrag {
                break;
            }
        }
        cset.sort_by_cached_key(|r| r.defrag_blocks().count());
        immix.immix_space.collection_set.set_reigons(cset);
    }
}

impl<VM: VMBinding> DefragPolicy<VM> for EagerDefragPolicy {
    fn select(&'static self, mmtk: &'static MMTK<VM>) {
        assert!(crate::args::LXR_EAGER_DEFRAG_SELECTION);
        let threshold =
            crate::args::LXR_DEFRAG_BLOCK_LIVENESS_THRESHOLD.unwrap() * Block::BYTES / 100;
        let immix = mmtk.plan.downcast_ref::<Immix<VM>>().unwrap();
        let space = &immix.immix_space;
        self.fragmented_blocks_size.store(0, Ordering::SeqCst);
        space.run_per_chunk_tasks(
            "CollectRegions",
            move |chunk| {
                let mut blocks = vec![];
                // Calculate score
                if *crate::args::FRAG_BLOCK_SELECTION {
                    for block in chunk.committed_mature_blocks() {
                        let holes = block.calc_holes();
                        if holes > 2 {
                            blocks.push((block, Block::LINES * 100 / holes));
                        }
                    }
                } else {
                    for block in chunk.committed_mature_blocks() {
                        let live_bytes =
                            (Block::LINES - block.calc_dead_lines()) << Line::LOG_BYTES;
                        if live_bytes <= threshold {
                            blocks.push((block, live_bytes))
                        }
                    }
                }
                self.fragmented_blocks_size
                    .fetch_add(blocks.len(), Ordering::SeqCst);
                self.fragmented_blocks.push(blocks);
            },
            move || {
                self.select_mature_evacuation_candidates(
                    immix.current_pause().unwrap(),
                    mmtk.plan.get_total_pages(),
                    &immix,
                )
            },
        );
    }

    fn schedule(&self, _cset: &CollectionSet, regions: &mut Vec<Region>) -> RegionSelection {
        if cfg!(feature = "lxr_region_inf") {
            return CollectionSet::take_all_regions(regions);
        }
        CollectionSet::incremental_test(regions)?;
        CollectionSet::time_limit_test()?;
        let mut total_copy_bytes = 0usize;
        let threshold = *crate::args::LXR_REGION_COALESCE_THRESHOLD << 20;
        CollectionSet::filter_prioritized_regions(regions, |region| {
            for b in region.defrag_blocks() {
                total_copy_bytes += b.live_bytes();
            }
            total_copy_bytes <= threshold
        })
    }
}