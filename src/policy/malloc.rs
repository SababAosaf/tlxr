// a collection of functions and data structures used by MallocMS
// currently under policy so that is_malloced can be accessed by the OpenJDK binding
// once the sparse SFT table is in use and is_malloced is replaced by is_mapped_address, this should be moved to plan::mallocms

use crate::util::conversions;
use crate::util::heap::layout::vm_layout_constants::BYTES_IN_CHUNK;
use crate::util::side_metadata::SideMetadata;
use crate::util::side_metadata::SideMetadataID;
use crate::util::Address;
use crate::util::ObjectReference;
use std::collections::HashSet;
use std::sync::atomic::AtomicUsize;
use std::sync::RwLock;
use atomic::Ordering;
use conversions::chunk_align_down;

// Import calloc, free, and malloc_usable_size from the library specified in Cargo.toml:45
#[cfg(feature = "malloc_jemalloc")]
pub use jemalloc_sys::{calloc, free, malloc_usable_size};

#[cfg(feature = "malloc_mimalloc")]
pub use mimalloc_sys::{
    mi_calloc as calloc,
    mi_free as free,
    mi_malloc_usable_size as malloc_usable_size,
};

#[cfg(feature = "malloc_tcmalloc")]
pub use tcmalloc_sys::{
    TCMallocInternalCalloc as calloc,
    TCMallocInternalFree as free,
    TCMallocInternalMallocSize as malloc_usable_size,
};

// export LD_LIBRARY_PATH=/home/paiger/mmtk-openjdk/mmtk/target/release/build/hoard-sys-bcc6c3e0a7e92343/out/Hoard/src
#[cfg(feature = "malloc_hoard")]
pub use hoard_sys::{calloc, free, malloc_usable_size};

// export LD_LIBRARY_PATH=/home/paiger/scalloc-sys/scalloc/out/Release/lib.target
#[cfg(feature = "malloc_scalloc")]
pub use scalloc_sys::{calloc, free, malloc_usable_size};

#[cfg(not(any(
    feature = "malloc_jemalloc",
    feature = "malloc_mimalloc",
    feature = "malloc_tcmalloc",
    feature = "malloc_hoard",
    feature = "malloc_scalloc"
)))]
pub use libc::{calloc, free, malloc_usable_size};

lazy_static! {
    pub static ref MAPPED_CHUNKS: RwLock<HashSet<Address>> = RwLock::default();
}

pub static mut HEAP_SIZE: usize = 0;
pub static HEAP_USED: AtomicUsize = AtomicUsize::new(0);
pub static mut ALLOCATION_METADATA_ID: SideMetadataID = SideMetadataID::new();
pub static mut MARKING_METADATA_ID: SideMetadataID = SideMetadataID::new();

pub fn heap_full() -> bool {
    unsafe { HEAP_USED.load(Ordering::SeqCst) >= HEAP_SIZE }
}

pub fn meta_space_mapped(address: Address) -> bool {
    let chunk_start = chunk_align_down(address);
    MAPPED_CHUNKS.read().unwrap().contains(&chunk_start)
}

pub unsafe fn map_meta_space_for_chunk(chunk_start: Address) {
    SideMetadata::map_meta_space(chunk_start, BYTES_IN_CHUNK, ALLOCATION_METADATA_ID);
    SideMetadata::map_meta_space(chunk_start, BYTES_IN_CHUNK, MARKING_METADATA_ID);
    MAPPED_CHUNKS.write().unwrap().insert(chunk_start);
}

// Check if a given object was allocated by malloc
pub fn is_malloced(object: ObjectReference) -> bool {
    let address = object.to_address();
    unsafe { meta_space_mapped(address) && SideMetadata::load_atomic(ALLOCATION_METADATA_ID, address) == 1 }
}

// check the corresponding bit in the metadata table
pub fn is_marked(object: ObjectReference) -> bool {
    let address = object.to_address();
    debug_assert!(meta_space_mapped(address));
    unsafe { SideMetadata::load_atomic(MARKING_METADATA_ID, address) == 1 }
}

pub fn set_alloc_bit(address: Address) {
    debug_assert!(meta_space_mapped(address));
    unsafe {
        SideMetadata::store_atomic(ALLOCATION_METADATA_ID, address, 1);
    }
}

pub fn set_mark_bit(address: Address) {
    debug_assert!(meta_space_mapped(address));
    unsafe {
        SideMetadata::store_atomic(MARKING_METADATA_ID, address, 1);
    }
}

pub fn unset_alloc_bit(address: Address) {
    debug_assert!(meta_space_mapped(address));
    unsafe {
        SideMetadata::store_atomic(ALLOCATION_METADATA_ID, address, 0);
    }
}

pub fn unset_mark_bit(address: Address) {
    debug_assert!(meta_space_mapped(address));
    unsafe {
        SideMetadata::store_atomic(MARKING_METADATA_ID, address, 0);
    }
}
