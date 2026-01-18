use core::alloc::Layout;
use core::ptr::NonNull;

use crate::cache::Cache;
use crate::page_provider::PageProvider;

pub const SIZE_CLASSES: [usize; 9] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];

pub struct SlabAllocator<P: PageProvider> {
    provider: P,
    caches: [Cache; 9],
}

impl<P: PageProvider> SlabAllocator<P> {
    pub fn new(provider: P) -> Self {
        let caches = [
            Cache::new(8, 8),
            Cache::new(16, 16),
            Cache::new(32, 32),
            Cache::new(64, 64),
            Cache::new(128, 128),
            Cache::new(256, 256),
            Cache::new(512, 512),
            Cache::new(1024, 1024),
            Cache::new(2048, 2048),
        ];

        Self { provider, caches }
    }

    #[inline]
    fn class_index(size: usize) -> Option<usize> {
        SIZE_CLASSES.iter().position(|&c| c >= size)
    }

    #[inline]
    fn pick_index(layout: Layout) -> Option<usize> {
        let size = layout.size().max(1);
        let align = layout.align();

        let idx = Self::class_index(size)?;

        // règle minimale: align <= sizeclass
        if align > SIZE_CLASSES[idx] {
            return None;
        }

        Some(idx)
    }

    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let Some(idx) = Self::pick_index(layout) else {
            return core::ptr::null_mut();
        };

        // Emprunts séparés => plus de E0499
        let provider = &mut self.provider;
        let cache = &mut self.caches[idx];

        match cache.alloc(provider) {
            Some(p) => p.as_ptr(),
            None => core::ptr::null_mut(),
        }
    }

    /// # Safety
    /// - `ptr` doit provenir d’un `alloc(layout)` de CET allocator.
    /// - `layout` doit être identique à celui utilisé lors de l'allocation (même size/align).
    /// - pas de double-free.
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }

        let Some(idx) = Self::pick_index(layout) else {
            debug_assert!(false, "dealloc: unsupported layout");
            return;
        };

        let cache = &mut self.caches[idx];
        let nn = NonNull::new(ptr).expect("ptr checked non-null above");
	// SAFETY:
	// - ptr provient d’un alloc(layout) de CET allocator (précondition de dealloc)
	// - layout route vers ce cache (pick_index identique)
	// - pas de double free (précondition)
	unsafe { cache.dealloc(nn) };
    }

    pub fn provider_mut(&mut self) -> &mut P {
        &mut self.provider
    }
}