initSidebarItems({"constant":[["ACTIVE_BARRIER","Barrier overhead measurement:"],["FULL_NURSERY_GC","Full heap collection as nursery GC."],["GEN_CONSTRAINTS","Constraints for generational plans. Each generational plan should overwrite based on this constant."],["NO_SLOW","Force object barrier never enters the slow-path. If enabled,"]],"fn":[["generational_post_copy","Post copying operation for generational plans."],["new_generational_global_metadata_specs","Create global side metadata specs for generational plans. This will call SideMetadataContext::new_global_specs(). So if a plan calls this, it should not call SideMetadataContext::new_global_specs() again."]],"mod":[["copying","Generational copying (GenCopy) Plan: generational copying"],["gc_work",""],["global",""]]});