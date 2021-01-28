(function() {var implementors = {};
implementors["mmtk"] = [{"text":"impl Sync for Address","synthetic":true,"types":[]},{"text":"impl Sync for ObjectReference","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Sync for Allocators&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Sync for AllocatorSelector","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Sync for BumpAllocator&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Sync for DumpLinearScan","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Sync for LargeObjectAllocator&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Sync for GCByte","synthetic":true,"types":[]},{"text":"impl Sync for FragmentedMapper","synthetic":true,"types":[]},{"text":"impl Sync for Map64","synthetic":true,"types":[]},{"text":"impl Sync for CommonFreeListPageResource","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Sync for FreeListPageResource&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Sync for HeapMeta","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Sync for MonotonePageResource&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Sync for MonotonePageResourceConditional","synthetic":true,"types":[]},{"text":"impl&lt;VM&gt; Sync for CommonPageResource&lt;VM&gt;","synthetic":true,"types":[]},{"text":"impl Sync for SpaceDescriptor","synthetic":true,"types":[]},{"text":"impl Sync for VMRequest","synthetic":true,"types":[]},{"text":"impl Sync for IntArrayFreeList","synthetic":true,"types":[]},{"text":"impl Sync for NurseryZeroingOptions","synthetic":true,"types":[]},{"text":"impl Sync for Options","synthetic":true,"types":[]},{"text":"impl Sync for RawMemoryFreeList","synthetic":true,"types":[]},{"text":"impl Sync for ReferenceProcessors","synthetic":true,"types":[]},{"text":"impl Sync for Semantics","synthetic":true,"types":[]},{"text":"impl Sync for MonotoneNanoTime","synthetic":true,"types":[]},{"text":"impl&lt;T&gt; Sync for LongCounter&lt;T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;&lt;T as Diffable&gt;::Val: Sync,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Sync for SharedStats","synthetic":true,"types":[]},{"text":"impl Sync for Stats","synthetic":true,"types":[]},{"text":"impl Sync for SynchronizedCounter","synthetic":true,"types":[]},{"text":"impl Sync for TreadMill","synthetic":true,"types":[]},{"text":"impl Sync for AllocationSemantics","synthetic":true,"types":[]},{"text":"impl&lt;P&gt; Sync for Mutator&lt;P&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;&lt;P as Plan&gt;::VM: VMBinding,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl Sync for SpaceOptions","synthetic":true,"types":[]},{"text":"impl&lt;C&gt; Sync for CoordinatorMessage&lt;C&gt;","synthetic":true,"types":[]},{"text":"impl Sync for SchedulerStat","synthetic":true,"types":[]},{"text":"impl Sync for WorkStat","synthetic":true,"types":[]},{"text":"impl Sync for WorkerLocalStat","synthetic":true,"types":[]},{"text":"impl Sync for WorkBucketStage","synthetic":true,"types":[]},{"text":"impl&lt;C&gt; Sync for WorkerGroup&lt;C&gt;","synthetic":true,"types":[]},{"text":"impl Sync for PrepareCollector","synthetic":true,"types":[]},{"text":"impl Sync for ReleaseCollector","synthetic":true,"types":[]},{"text":"impl&lt;ScanEdges&gt; Sync for StopMutators&lt;ScanEdges&gt;","synthetic":true,"types":[]},{"text":"impl Sync for EndOfGC","synthetic":true,"types":[]},{"text":"impl&lt;Edges&gt; Sync for ScanStackRoots&lt;Edges&gt;","synthetic":true,"types":[]},{"text":"impl&lt;Edges&gt; Sync for ScanStackRoot&lt;Edges&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;&lt;Edges as ProcessEdgesWork&gt;::VM: VMBinding,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;Edges&gt; Sync for ScanVMSpecificRoots&lt;Edges&gt;","synthetic":true,"types":[]},{"text":"impl&lt;Edges&gt; Sync for ScanObjects&lt;Edges&gt;","synthetic":true,"types":[]},{"text":"impl&lt;E&gt; Sync for ProcessModBuf&lt;E&gt;","synthetic":true,"types":[]},{"text":"impl Sync for OpaquePointer","synthetic":false,"types":[]},{"text":"impl Sync for UnsafeOptionsWrapper","synthetic":false,"types":[]},{"text":"impl Sync for ReferenceProcessor","synthetic":false,"types":[]},{"text":"impl&lt;VM:&nbsp;VMBinding&gt; Sync for MMTK&lt;VM&gt;","synthetic":false,"types":[]},{"text":"impl Sync for SFTMap","synthetic":false,"types":[]},{"text":"impl&lt;VM:&nbsp;VMBinding&gt; Sync for CommonSpace&lt;VM&gt;","synthetic":false,"types":[]},{"text":"impl&lt;VM:&nbsp;VMBinding&gt; Sync for ImmortalSpace&lt;VM&gt;","synthetic":false,"types":[]},{"text":"impl&lt;VM:&nbsp;VMBinding&gt; Sync for CopySpace&lt;VM&gt;","synthetic":false,"types":[]},{"text":"impl&lt;VM:&nbsp;VMBinding&gt; Sync for LargeObjectSpace&lt;VM&gt;","synthetic":false,"types":[]},{"text":"impl&lt;C:&nbsp;Context&gt; Sync for Scheduler&lt;C&gt;","synthetic":false,"types":[]},{"text":"impl&lt;C:&nbsp;Context&gt; Sync for Worker&lt;C&gt;","synthetic":false,"types":[]},{"text":"impl Sync for ScheduleCollection","synthetic":false,"types":[]},{"text":"impl&lt;P:&nbsp;Plan&gt; Sync for Prepare&lt;P&gt;","synthetic":false,"types":[]},{"text":"impl&lt;VM:&nbsp;VMBinding&gt; Sync for PrepareMutator&lt;VM&gt;","synthetic":false,"types":[]},{"text":"impl&lt;P:&nbsp;Plan&gt; Sync for Release&lt;P&gt;","synthetic":false,"types":[]},{"text":"impl&lt;VM:&nbsp;VMBinding&gt; Sync for ReleaseMutator&lt;VM&gt;","synthetic":false,"types":[]},{"text":"impl&lt;E:&nbsp;ProcessEdgesWork&gt; Sync for ProcessEdgesBase&lt;E&gt;","synthetic":false,"types":[]}];
if (window.register_implementors) {window.register_implementors(implementors);} else {window.pending_implementors = implementors;}})()