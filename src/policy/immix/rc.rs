use atomic::Ordering;

use crate::util::{
    metadata::side_metadata::{self, SideMetadataOffset, SideMetadataSpec},
    ObjectReference,
};

use super::chunk::ChunkMap;

const LOG_REF_COUNT_BITS: usize = 3;
const MAX_REF_COUNT: usize = (1 << (1 << LOG_REF_COUNT_BITS)) - 1;

pub const RC_TABLE: SideMetadataSpec = SideMetadataSpec {
    is_global: false,
    offset: SideMetadataOffset::layout_after(&ChunkMap::ALLOC_TABLE),
    log_num_of_bits: LOG_REF_COUNT_BITS,
    log_min_obj_size: 3,
};

#[inline(always)]
pub fn inc(o: ObjectReference) -> Result<usize, usize> {
    debug_assert!(!o.is_null());
    let r = side_metadata::fetch_update(
        &RC_TABLE,
        o.to_address(),
        Ordering::SeqCst,
        Ordering::SeqCst,
        |x| {
            if x == MAX_REF_COUNT {
                None
            } else {
                Some(x + 1)
            }
        },
    );
    // println!("inc {:?} {:?}", o, count(o));
    r
}

#[inline(always)]
pub fn dec(o: ObjectReference) -> Result<usize, usize> {
    debug_assert!(!o.is_null());
    let r = side_metadata::fetch_update(
        &RC_TABLE,
        o.to_address(),
        Ordering::SeqCst,
        Ordering::SeqCst,
        |x| {
            if x == 0 || x == MAX_REF_COUNT
            /* sticky */
            {
                None
            } else {
                Some(x - 1)
            }
        },
    );
    // println!("dec {:?} {:?}", o, count(o));
    r
}

pub fn count(o: ObjectReference) -> usize {
    side_metadata::load_atomic(&RC_TABLE, o.to_address(), Ordering::SeqCst)
}

pub fn is_dead(o: ObjectReference) -> bool {
    let v = side_metadata::load_atomic(&RC_TABLE, o.to_address(), Ordering::SeqCst);
    v == 0
}

#[inline(always)]
pub fn reset(o: ObjectReference) {
    debug_assert!(!o.is_null());
    side_metadata::store_atomic(&RC_TABLE, o.to_address(), 0, Ordering::SeqCst)
}