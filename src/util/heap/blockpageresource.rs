use super::pageresource::{PRAllocFail, PRAllocResult};
use super::PageResource;
use crate::util::address::Address;
use crate::util::constants::*;
use crate::util::heap::layout::heap_layout::VMMap;
use crate::util::heap::layout::vm_layout_constants::*;
use crate::util::heap::pageresource::CommonPageResource;
use crate::util::heap::space_descriptor::SpaceDescriptor;
use crate::util::opaque_pointer::*;
use crate::vm::*;
use atomic::{Atomic, Ordering};
use spin::rwlock::RwLock;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::atomic::AtomicUsize;
use std::sync::Mutex;

const UNINITIALIZED_WATER_MARK: i32 = -1;
const LOCAL_BUFFER_SIZE: usize = 128;

pub struct BlockPageResource<VM: VMBinding> {
    common: CommonPageResource,
    log_pages: usize,
    sync: Mutex<()>,
    block_queue: BlockQueue<Address>,
    highwater: Atomic<Address>,
    limit: Address,
    _p: PhantomData<VM>,
}

impl<VM: VMBinding> PageResource<VM> for BlockPageResource<VM> {
    #[inline(always)]
    fn common(&self) -> &CommonPageResource {
        &self.common
    }

    #[inline(always)]
    fn common_mut(&mut self) -> &mut CommonPageResource {
        &mut self.common
    }

    #[inline(always)]
    fn alloc_pages(
        &self,
        space_descriptor: SpaceDescriptor,
        reserved_pages: usize,
        required_pages: usize,
        tls: VMThread,
    ) -> Result<PRAllocResult, PRAllocFail> {
        let _sync = self.sync.lock().unwrap();
        unsafe { self.alloc_pages_no_lock(space_descriptor, reserved_pages, required_pages, tls) }
    }

    #[inline(always)]
    fn adjust_for_metadata(&self, pages: usize) -> usize {
        pages
    }

    #[inline(always)]
    fn bpr(&self) -> Option<&BlockPageResource<VM>> {
        Some(self)
    }
}

impl<VM: VMBinding> BlockPageResource<VM> {
    pub fn new_contiguous(
        log_pages: usize,
        start: Address,
        bytes: usize,
        vm_map: &'static VMMap,
    ) -> Self {
        let growable = cfg!(target_pointer_width = "64");
        assert!((1 << log_pages) <= PAGES_IN_CHUNK);
        Self {
            log_pages,
            common: CommonPageResource::new(true, growable, vm_map),
            sync: Mutex::new(()),
            highwater: Atomic::new(start),
            limit: (start + bytes).align_up(BYTES_IN_CHUNK),
            block_queue: BlockQueue::new(),
            _p: PhantomData,
        }
    }

    /// The caller needs to ensure this is called by only one thread.
    #[inline]
    pub unsafe fn alloc_pages_no_lock(
        &self,
        _space_descriptor: SpaceDescriptor,
        reserved_pages: usize,
        required_pages: usize,
        tls: VMThread,
    ) -> Result<PRAllocResult, PRAllocFail> {
        debug_assert_eq!(reserved_pages, required_pages);
        debug_assert_eq!(reserved_pages, 1 << self.log_pages);
        // Fast allocate from the blocks list
        if let Some(block) = self.block_queue.pop() {
            self.commit_pages(reserved_pages, required_pages, tls);
            return Result::Ok(PRAllocResult {
                start: block,
                pages: required_pages,
                new_chunk: false,
            });
        }
        // Grow space
        let start: Address =
            match self
                .highwater
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| {
                    if x >= self.limit {
                        None
                    } else {
                        Some(x + BYTES_IN_CHUNK)
                    }
                }) {
                Ok(a) => a,
                _ => return Result::Err(PRAllocFail),
            };
        assert!(start.is_aligned_to(BYTES_IN_CHUNK));
        let first_block = start;
        let last_block = start + BYTES_IN_CHUNK;
        let block_size = 1usize << (self.log_pages + LOG_BYTES_IN_PAGE as usize);
        let array = BlockArray::new();
        let mut cursor = start + block_size;
        while cursor < last_block {
            array.push_relaxed(cursor).unwrap();
            cursor = cursor + block_size;
        }
        self.block_queue.add_global_array(array);
        self.commit_pages(reserved_pages, required_pages, tls);
        Result::Ok(PRAllocResult {
            start: first_block,
            pages: PAGES_IN_CHUNK,
            new_chunk: true,
        })
    }

    #[inline]
    pub fn release_pages(&self, first: Address) {
        debug_assert!(self.common.contiguous);
        debug_assert!(first.is_aligned_to(1usize << (self.log_pages + LOG_BYTES_IN_PAGE as usize)));
        let pages = 1 << self.log_pages;
        debug_assert!(pages as usize <= self.common.accounting.get_committed_pages());
        self.common.accounting.release(pages as _);
        self.block_queue.push(first)
    }

    pub fn flush(&self, id: usize) {
        self.block_queue.flush(id)
    }

    pub fn flush_all(&self) {
        self.block_queue.flush_all()
    }
}

struct BlockArray<Block> {
    cursor: AtomicUsize,
    data: Vec<Block>,
    capacity: usize,
}

impl<Block: Copy> BlockArray<Block> {
    const LOCAL_BUFFER_SIZE: usize = 256;

    #[inline(always)]
    fn new() -> Self {
        let mut array = Self {
            cursor: AtomicUsize::new(0),
            data: Vec::with_capacity(Self::LOCAL_BUFFER_SIZE),
            capacity: Self::LOCAL_BUFFER_SIZE,
        };
        unsafe { array.data.set_len(Self::LOCAL_BUFFER_SIZE) }
        array
    }

    #[inline(always)]
    fn data(&self) -> &mut Vec<Block> {
        unsafe { &mut (*(self as *const Self as *mut Self)).data }
    }

    #[inline(always)]
    fn push_relaxed(&self, block: Block) -> Result<(), Block> {
        let i = self.cursor.load(Ordering::Relaxed);
        if i < self.capacity {
            self.data()[i] = block;
            self.cursor.store(i + 1, Ordering::Relaxed);
            Ok(())
        } else {
            Err(block)
        }
    }

    #[inline(always)]
    fn pop(&self) -> Option<Block> {
        let i = self
            .cursor
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |i| {
                if i > 0 {
                    Some(i - 1)
                } else {
                    None
                }
            });
        if let Ok(i) = i {
            Some(self.data()[i - 1])
        } else {
            None
        }
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.cursor.load(Ordering::Relaxed)
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    fn iterate_blocks(&self, f: &mut impl FnMut(Block)) {
        let len = self.len();
        for i in 0..len {
            f(self.data()[i])
        }
    }
}

pub struct BlockQueue<Block> {
    head_global_freed_blocks: RwLock<Option<BlockArray<Block>>, spin::Yield>,
    global_freed_blocks: RwLock<Vec<BlockArray<Block>>, spin::Yield>,
    worker_local_freed_blocks: Vec<RwLock<BlockArray<Block>, spin::Yield>>,
    count: AtomicUsize,
}

impl<Block: Debug + Copy> BlockQueue<Block> {
    pub fn new() -> Self {
        let mut worker_local_freed_blocks = vec![];
        worker_local_freed_blocks.resize_with(*crate::CALC_WORKERS, || {
            spin::rwlock::RwLock::new(BlockArray::new())
        });
        Self {
            head_global_freed_blocks: Default::default(),
            global_freed_blocks: Default::default(),
            worker_local_freed_blocks,
            count: AtomicUsize::new(0),
        }
    }

    fn add_global_array(&self, array: BlockArray<Block>) {
        self.count.fetch_add(array.len(), Ordering::Relaxed);
        self.global_freed_blocks.write().push(array);
    }

    #[inline(always)]
    pub fn push(&self, block: Block) {
        self.count.fetch_add(1, Ordering::Relaxed);
        let id = crate::gc_worker_id().unwrap();
        let failed = self.worker_local_freed_blocks[id]
            .read()
            .push_relaxed(block)
            .is_err();
        if failed {
            let mut queue = BlockArray::new();
            {
                let mut lock = self.worker_local_freed_blocks[id].write();
                std::mem::swap(&mut queue, &mut *lock);
                lock.push_relaxed(block).unwrap();
            }
            if !queue.is_empty() {
                self.global_freed_blocks.write().push(queue);
            }
        }
    }

    #[inline(always)]
    pub fn pop(&self) -> Option<Block> {
        let head_global_freed_blocks = self.head_global_freed_blocks.upgradeable_read();
        if let Some(block) = head_global_freed_blocks.as_ref().map(|q| q.pop()).flatten() {
            self.count.fetch_sub(1, Ordering::Relaxed);
            Some(block)
        } else if let Some(blocks) = self.global_freed_blocks.write().pop() {
            let block = blocks.pop().unwrap();
            if !blocks.is_empty() {
                let mut head_global_freed_blocks = head_global_freed_blocks.upgrade();
                *head_global_freed_blocks = Some(blocks);
            }
            self.count.fetch_sub(1, Ordering::Relaxed);
            Some(block)
        } else {
            None
        }
    }

    pub fn flush(&self, id: usize) {
        let read = self.worker_local_freed_blocks[id].upgradeable_read();
        if !read.is_empty() {
            let mut queue = BlockArray::new();
            let mut write = read.upgrade();
            std::mem::swap(&mut queue, &mut *write);
            if !queue.is_empty() {
                self.global_freed_blocks.write().push(queue)
            }
        }
    }

    pub fn flush_all(&self) {
        for i in 0..self.worker_local_freed_blocks.len() {
            self.flush(i)
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn iterate_blocks(&self, f: &mut impl FnMut(Block)) {
        self.head_global_freed_blocks
            .read()
            .as_ref()
            .map(|array| array.iterate_blocks(f));
        for array in &*self.global_freed_blocks.read() {
            array.iterate_blocks(f);
        }
        for array in &self.worker_local_freed_blocks {
            array.read().iterate_blocks(f);
        }
    }
}
