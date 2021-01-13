use libc::{size_t, c_void};

#[cfg(feature = "malloc_scalloc")]
#[link(name = "scalloc")]
extern "C" {
    pub fn calloc(n: size_t, size: size_t) -> *mut c_void;
    pub fn free(p: *mut c_void);
    pub fn malloc_usable_size(p: *mut c_void) -> size_t;
}