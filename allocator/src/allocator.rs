use core::alloc::Layout;
use core::ptr::NonNull;

use crate::cache::Cache;
use crate::page_provider::PageProvider;

const SIZE_CLASSES: [usize; 9] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];

pub struct SlabAllocator<P: PageProvider> {
    provider: P,
    caches: [Cache; 9],
}

impl<P: PageProvider> SlabAllocator<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            caches: [
                Cache::new(8),
                Cache::new(16),
                Cache::new(32),
                Cache::new(64),
                Cache::new(128),
                Cache::new(256),
                Cache::new(512),
                Cache::new(1024),
                Cache::new(2048),
            ],
        }
    }

    fn cache_index(layout: Layout) -> Option<usize> {
        let size = layout.size();
        let align = layout.align();

        // on exige que la size class >= size et >= align
        SIZE_CLASSES.iter().position(|&c| c >= size && c >= align)
    }

    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let Some(i) = Self::cache_index(layout) else {
            return core::ptr::null_mut();
        };

        match self.caches[i].alloc(&mut self.provider) {
            Some(p) => p.as_ptr(),
            None => core::ptr::null_mut(),
        }
    }

    pub fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let Some(nn) = NonNull::new(ptr) else { return; };

        let Some(i) = Self::cache_index(layout) else {
            // taille non supportée -> ignore (ou panic en debug)
            return;
        };

        self.caches[i].dealloc(&mut self.provider, nn);
    }
}

pub fn alloc(_layout: Layout) -> *mut u8 {
    core::ptr::null_mut()
}

/// API globale minimale (router).
/// Pour l'instant: stub (no-op).
pub fn dealloc(_ptr: *mut u8, _layout: Layout) {
    // no-op
}
