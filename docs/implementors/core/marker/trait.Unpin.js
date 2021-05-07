(function() {var implementors = {};
implementors["mmtk"] = [{"text":"impl Unpin for Address","synthetic":true,"types":[]},{"text":"impl Unpin for ObjectReference","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for Allocators&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Unpin for AllocatorSelector","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for BumpAllocator&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Unpin for DumpLinearScan","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for LargeObjectAllocator&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for MallocAllocator&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Unpin for GcCounter","synthetic":true,"types":[]},{"text":"impl Unpin for ObjectCounter","synthetic":true,"types":[]},{"text":"impl Unpin for PerSizeClassObjectCounter","synthetic":true,"types":[]},{"text":"impl Unpin for GcHookWork","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for AnalysisManager&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Unpin for FinalizableProcessor","synthetic":true,"types":[]},{"text":"impl&lt;E&gt; Unpin for Finalization&lt;E&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;E: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;E&gt; Unpin for ForwardFinalization&lt;E&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;E: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Unpin for HeaderByte","synthetic":true,"types":[]},{"text":"impl Unpin for PageAccounting","synthetic":true,"types":[]},{"text":"impl Unpin for FragmentedMapper","synthetic":true,"types":[]},{"text":"impl Unpin for Map64","synthetic":true,"types":[]},{"text":"impl Unpin for CommonFreeListPageResource","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for FreeListPageResource&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Unpin for HeapMeta","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for MonotonePageResource&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Unpin for MonotonePageResourceConditional","synthetic":true,"types":[]},{"text":"impl Unpin for PRAllocResult","synthetic":true,"types":[]},{"text":"impl Unpin for PRAllocFail","synthetic":true,"types":[]},{"text":"impl Unpin for CommonPageResource","synthetic":true,"types":[]},{"text":"impl Unpin for SpaceDescriptor","synthetic":true,"types":[]},{"text":"impl Unpin for VMRequest","synthetic":true,"types":[]},{"text":"impl Unpin for IntArrayFreeList","synthetic":true,"types":[]},{"text":"impl Unpin for OpaquePointer","synthetic":true,"types":[]},{"text":"impl Unpin for NurseryZeroingOptions","synthetic":true,"types":[]},{"text":"impl Unpin for PlanSelector","synthetic":true,"types":[]},{"text":"impl Unpin for UnsafeOptionsWrapper","synthetic":true,"types":[]},{"text":"impl Unpin for Options","synthetic":true,"types":[]},{"text":"impl Unpin for RawMemoryFreeList","synthetic":true,"types":[]},{"text":"impl Unpin for ReferenceProcessors","synthetic":true,"types":[]},{"text":"impl Unpin for ReferenceProcessor","synthetic":true,"types":[]},{"text":"impl Unpin for Semantics","synthetic":true,"types":[]},{"text":"impl Unpin for SanityChecker","synthetic":true,"types":[]},{"text":"impl&lt;P, W&gt; Unpin for ScheduleSanityGC&lt;P, W&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;W: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;P, W&gt; Unpin for SanityPrepare&lt;P, W&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;W: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;P, W&gt; Unpin for SanityRelease&lt;P, W&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;W: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for SanityGCProcessEdges&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Unpin for SideMetadataScope","synthetic":true,"types":[]},{"text":"impl Unpin for SideMetadataSpec","synthetic":true,"types":[]},{"text":"impl Unpin for SideMetadataContext","synthetic":true,"types":[]},{"text":"impl Unpin for SideMetadata","synthetic":true,"types":[]},{"text":"impl Unpin for EventCounter","synthetic":true,"types":[]},{"text":"impl&lt;T&gt; Unpin for LongCounter&lt;T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;&lt;T as Diffable&gt;::Val: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Unpin for SizeCounter","synthetic":true,"types":[]},{"text":"impl Unpin for MonotoneNanoTime","synthetic":true,"types":[]},{"text":"impl Unpin for SharedStats","synthetic":true,"types":[]},{"text":"impl Unpin for Stats","synthetic":true,"types":[]},{"text":"impl Unpin for SynchronizedCounter","synthetic":true,"types":[]},{"text":"impl Unpin for TreadMill","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for MMTK&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Unpin for BarrierSelector","synthetic":true,"types":[]},{"text":"impl Unpin for WriteTarget","synthetic":true,"types":[]},{"text":"impl Unpin for NoBarrier","synthetic":true,"types":[]},{"text":"impl&lt;E&gt; Unpin for ObjectRememberingBarrier&lt;E&gt;","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for ControllerCollectorContext&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for NoCopy&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Unpin for GcStatus","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for BasePlan&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for BaseUnsync&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for CommonPlan&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for CommonUnsync&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Unpin for AllocationSemantics","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for MutatorConfig&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for Mutator&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Unpin for PlanConstraints","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for GenCopy&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for MarkSweep&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Unpin for ALLOCATOR_MAPPING","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for NoGC&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for SemiSpace&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'a&gt; Unpin for SFTMap&lt;'a&gt;","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for CommonSpace&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Unpin for SpaceOptions","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for CopySpace&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for ImmortalSpace&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for LargeObjectSpace&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for LockFreeImmortalSpace&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for MallocSpace&lt;VM&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;VM: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Unpin for ACTIVE_CHUNKS","synthetic":true,"types":[]},{"text":"impl Unpin for ALLOC_MAP","synthetic":true,"types":[]},{"text":"impl Unpin for MARK_MAP","synthetic":true,"types":[]},{"text":"impl&lt;C&gt; Unpin for CoordinatorMessage&lt;C&gt;","synthetic":true,"types":[]},{"text":"impl&lt;C&gt; Unpin for Scheduler&lt;C&gt;","synthetic":true,"types":[]},{"text":"impl Unpin for SchedulerStat","synthetic":true,"types":[]},{"text":"impl Unpin for WorkStat","synthetic":true,"types":[]},{"text":"impl Unpin for WorkerLocalStat","synthetic":true,"types":[]},{"text":"impl Unpin for WorkBucketStage","synthetic":true,"types":[]},{"text":"impl Unpin for WorkerLocalPtr","synthetic":true,"types":[]},{"text":"impl&lt;C&gt; Unpin for Worker&lt;C&gt;","synthetic":true,"types":[]},{"text":"impl&lt;C&gt; Unpin for WorkerGroup&lt;C&gt;","synthetic":true,"types":[]},{"text":"impl Unpin for ScheduleCollection","synthetic":true,"types":[]},{"text":"impl&lt;P, W&gt; Unpin for Prepare&lt;P, W&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;W: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for PrepareMutator&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl&lt;W&gt; Unpin for PrepareCollector&lt;W&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;W: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;P, W&gt; Unpin for Release&lt;P, W&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;W: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Unpin for ReleaseMutator&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl&lt;W&gt; Unpin for ReleaseCollector&lt;W&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;W: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;ScanEdges&gt; Unpin for StopMutators&lt;ScanEdges&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;ScanEdges: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Unpin for EndOfGC","synthetic":true,"types":[]},{"text":"impl&lt;Edges&gt; Unpin for ScanStackRoots&lt;Edges&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;Edges: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;Edges&gt; Unpin for ScanStackRoot&lt;Edges&gt;","synthetic":true,"types":[]},{"text":"impl&lt;Edges&gt; Unpin for ScanVMSpecificRoots&lt;Edges&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;Edges: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;E&gt; Unpin for ProcessEdgesBase&lt;E&gt;","synthetic":true,"types":[]},{"text":"impl&lt;Edges&gt; Unpin for ScanObjects&lt;Edges&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;Edges: Unpin,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;E&gt; Unpin for ProcessModBuf&lt;E&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;E: Unpin,&nbsp;</span>","synthetic":true,"types":[]}];
if (window.register_implementors) {window.register_implementors(implementors);} else {window.pending_implementors = implementors;}})()