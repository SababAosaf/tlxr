mod barrier;
pub(super) mod cm;
mod gc_work;
pub(super) mod global;
mod mature_evac;
pub(super) mod mutator;
pub mod rc;
mod remset;

use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Mutex;
use std::time::SystemTime;

pub use self::global::LXR;
pub use self::global::LXR_CONSTRAINTS;
pub use self::remset::RemSet;

use atomic::Atomic;
use atomic::Ordering;
use crossbeam::queue::SegQueue;

use crate::util::Address;
use crate::util::ObjectReference;
use crate::vm::edge_shape::Edge;

const CYCLE_TRIGGER_THRESHOLD: usize = crate::args::CYCLE_TRIGGER_THRESHOLD;

pub static mut PREV_ROOTS: SegQueue<Vec<ObjectReference>> = SegQueue::new();
pub static mut CURR_ROOTS: SegQueue<Vec<ObjectReference>> = SegQueue::new();

pub static SURVIVAL_RATIO_PREDICTOR: SurvivalRatioPredictor = SurvivalRatioPredictor {
    prev_ratio: Atomic::new(0.2),
    alloc_vol: AtomicUsize::new(0),
    promote_vol: AtomicUsize::new(0),
    pause_start: Atomic::new(SystemTime::UNIX_EPOCH),
};

thread_local! {
    pub static SURVIVAL_RATIO_PREDICTOR_LOCAL: SurvivalRatioPredictorLocal =
        SurvivalRatioPredictorLocal {
            promote_vol: AtomicUsize::new(0),
        };
}

pub struct SurvivalRatioPredictor {
    prev_ratio: Atomic<f64>,
    alloc_vol: AtomicUsize,
    promote_vol: AtomicUsize,
    pub pause_start: Atomic<SystemTime>,
}

impl SurvivalRatioPredictor {
    #[inline(always)]
    pub fn set_alloc_size(&self, size: usize) {
        // println!("set_alloc_size {}", size);
        assert_eq!(self.alloc_vol.load(Ordering::Relaxed), 0);
        self.alloc_vol.store(size, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn ratio(&self) -> f64 {
        self.prev_ratio.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn update_ratio(&self) -> f64 {
        if self.alloc_vol.load(Ordering::Relaxed) == 0 {
            return self.ratio();
        }
        let prev = self.prev_ratio.load(Ordering::Relaxed);
        let curr = self.promote_vol.load(Ordering::Relaxed) as f64
            / self.alloc_vol.load(Ordering::Relaxed) as f64;
        let ratio = if *crate::args::SURVIVAL_PREDICTOR_WEIGHTED {
            if curr > prev {
                (curr * 3f64 + prev) / 4f64
            } else {
                (curr + 3f64 * prev) / 4f64
            }
        } else if *crate::args::SURVIVAL_PREDICTOR_HARMONIC_MEAN {
            if curr >= prev {
                2f64 * curr * prev / (curr + prev)
            } else {
                (curr * curr + prev * prev) / (curr + prev)
            }
        } else {
            (curr + prev) / 2f64
        };
        crate::add_survival_ratio(curr, prev);
        self.prev_ratio.store(ratio, Ordering::Relaxed);
        self.alloc_vol.store(0, Ordering::Relaxed);
        self.promote_vol.store(0, Ordering::Relaxed);
        ratio
    }
}

#[derive(Default)]
pub struct SurvivalRatioPredictorLocal {
    promote_vol: AtomicUsize,
}

impl SurvivalRatioPredictorLocal {
    #[inline(always)]
    pub fn record_promotion(&self, size: usize) {
        self.promote_vol.store(
            self.promote_vol.load(Ordering::Relaxed) + size,
            Ordering::Relaxed,
        );
    }

    #[inline(always)]
    pub fn sync(&self) {
        SURVIVAL_RATIO_PREDICTOR
            .promote_vol
            .fetch_add(self.promote_vol.load(Ordering::Relaxed), Ordering::Relaxed);
        self.promote_vol.store(0, Ordering::Relaxed);
    }
}

pub static MATURE_LIVE_PREDICTOR: MatureLivePredictor = MatureLivePredictor {
    live_pages: Atomic::new(0f64),
};

pub struct MatureLivePredictor {
    live_pages: Atomic<f64>,
}

impl MatureLivePredictor {
    #[inline(always)]
    pub fn live_pages(&self) -> f64 {
        self.live_pages.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn update(&self, live_pages: usize) {
        // println!("live_pages {}", live_pages);
        let prev = self.live_pages.load(Ordering::Relaxed);
        let curr = live_pages as f64;
        let weight = 3f64;
        let next = if curr > prev {
            (curr + prev * weight) / (weight + 1f64)
        } else {
            (weight * curr + prev) / (weight + 1f64)
        };
        // println!("predict {}", next);
        // crate::add_mature_reclaim(live_pages, prev);
        self.live_pages.store(next, Ordering::Relaxed);
    }
}

lazy_static! {
    static ref LAST_REFERENTS: Mutex<HashMap<Address, ObjectReference>> = Default::default();
}

#[inline(always)]
pub fn record_edge_for_validation(slot: impl Edge, obj: ObjectReference) {
    if cfg!(feature = "field_barrier_validation") {
        LAST_REFERENTS
            .lock()
            .unwrap()
            .insert(slot.to_address(), obj);
    }
}
