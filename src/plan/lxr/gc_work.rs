use super::cm::LXRWeakRefProcessEdges;
use super::LXR;
use crate::scheduler::{gc_work::*, GCWork, GCWorker};
use crate::{vm::*, Plan, MMTK};

pub(in crate::plan) type TraceKind = u8;
pub(in crate::plan) const TRACE_KIND_DEFAULT: TraceKind = 0;

pub(super) struct LXRGCWorkContext<VM: VMBinding>(std::marker::PhantomData<VM>);

impl<VM: VMBinding> crate::scheduler::GCWorkContext for LXRGCWorkContext<VM> {
    type VM = VM;
    type PlanType = LXR<VM>;
    type ProcessEdgesWorkType = PlanProcessEdges<VM, LXR<VM>, TRACE_KIND_DEFAULT>;
}

pub(super) struct LXRWeakRefWorkContext<VM: VMBinding, const COMPRESSED: bool>(
    std::marker::PhantomData<VM>,
);

impl<VM: VMBinding, const COMPRESSED: bool> crate::scheduler::GCWorkContext
    for LXRWeakRefWorkContext<VM, COMPRESSED>
{
    type VM = VM;
    type PlanType = LXR<VM>;
    type ProcessEdgesWorkType = LXRWeakRefProcessEdges<VM, COMPRESSED>;
}

pub struct FastRCPrepare;

impl<VM: VMBinding> GCWork<VM> for FastRCPrepare {
    fn do_work(&mut self, worker: &mut GCWorker<VM>, mmtk: &'static MMTK<VM>) {
        let lxr = mmtk.plan.downcast_ref::<LXR<VM>>().unwrap();
        let lxr = unsafe { &mut *(lxr as *const LXR<VM> as *mut LXR<VM>) };
        lxr.prepare(worker.tls)
    }
}