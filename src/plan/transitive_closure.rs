//! The fundamental mechanism for performing a transitive closure over an object graph.

use std::marker::PhantomData;
use std::mem;

use atomic::Ordering;

use crate::scheduler::gc_work::ProcessEdgesWork;
use crate::scheduler::{GCWorker, WorkBucketStage};
use crate::util::{Address, ObjectReference, VMThread, VMWorkerThread};
use crate::vm::{Scanning, VMBinding};
use crate::MMTK;

/// This trait is the fundamental mechanism for performing a
/// transitive closure over an object graph.
pub trait TransitiveClosure {
    // The signature of this function changes during the port
    // because the argument `ObjectReference source` is never used in the original version
    // See issue #5
    fn process_edge(&mut self, slot: Address);
    fn process_node(&mut self, object: ObjectReference);
}

impl<T: ProcessEdgesWork> TransitiveClosure for T {
    fn process_edge(&mut self, _slot: Address) {
        unreachable!();
    }
    #[inline]
    fn process_node(&mut self, object: ObjectReference) {
        ProcessEdgesWork::process_node(self, object);
    }
}

/// A transitive closure visitor to collect all the edges of an object.
pub struct ObjectsClosure<'a, E: ProcessEdgesWork> {
    mmtk: &'static MMTK<E::VM>,
    buffer: Vec<Address>,
    worker: &'a mut GCWorker<E::VM>,
    edge_bucket: WorkBucketStage,
}

impl<'a, E: ProcessEdgesWork> ObjectsClosure<'a, E> {
    pub fn new(
        mmtk: &'static MMTK<E::VM>,
        buffer: Vec<Address>,
        worker: &'a mut GCWorker<E::VM>,
        edge_bucket: WorkBucketStage,
    ) -> Self {
        Self {
            mmtk,
            buffer,
            worker,
            edge_bucket,
        }
    }
}

impl<'a, E: ProcessEdgesWork> TransitiveClosure for ObjectsClosure<'a, E> {
    #[inline(always)]
    fn process_edge(&mut self, slot: Address) {
        if self.buffer.is_empty() {
            self.buffer.reserve(E::CAPACITY);
        }
        self.buffer.push(slot);
        if self.buffer.len() >= E::CAPACITY {
            let mut new_edges = Vec::new();
            mem::swap(&mut new_edges, &mut self.buffer);
            self.worker
                .add_work(self.edge_bucket, E::new(new_edges, false, self.mmtk));
        }
    }
    fn process_node(&mut self, _object: ObjectReference) {
        unreachable!()
    }
}

impl<'a, E: ProcessEdgesWork> Drop for ObjectsClosure<'a, E> {
    #[inline(always)]
    fn drop(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let mut new_edges = Vec::new();
        mem::swap(&mut new_edges, &mut self.buffer);
        let w = E::new(new_edges, false, self.mmtk);
        if self.edge_bucket == WorkBucketStage::Unconstrained
            && self
                .worker
                .scheduler()
                .pause_concurrent_work_packets_during_gc
                .load(Ordering::SeqCst)
        {
            self.worker.scheduler().postpone(w);
        } else {
            self.worker.add_work(self.edge_bucket, w);
        }
    }
}

struct EdgeIteratorImpl<VM: VMBinding, F: FnMut(Address)> {
    f: F,
    _p: PhantomData<VM>,
}

impl<VM: VMBinding, F: FnMut(Address)> TransitiveClosure for EdgeIteratorImpl<VM, F> {
    #[inline(always)]
    fn process_edge(&mut self, slot: Address) {
        (self.f)(slot);
    }
    fn process_node(&mut self, _object: ObjectReference) {
        unreachable!()
    }
}

pub struct EdgeIterator<VM: VMBinding> {
    _p: PhantomData<VM>,
}

impl<VM: VMBinding> EdgeIterator<VM> {
    #[inline(always)]
    pub fn iterate(o: ObjectReference, f: impl FnMut(Address)) {
        let mut x = EdgeIteratorImpl::<VM, _> { f, _p: PhantomData };
        <VM::VMScanning as Scanning<VM>>::scan_object(
            &mut x,
            o,
            VMWorkerThread(VMThread::UNINITIALIZED),
        );
    }
}
