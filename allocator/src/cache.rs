use core::ptr::NonNull;

use crate::page_provider::PageProvider;
use crate::slab::{Slab, SlabHeader};

pub struct Cache {
    obj_size: usize,
    align: usize,
    head: Option<NonNull<SlabHeader>>,
}

impl Cache {
    pub const fn new(obj_size: usize, align: usize) -> Self {
        Self {
            obj_size,
            align,
            head: None,
        }
    }

    #[inline]
    pub fn obj_size(&self) -> usize {
        self.obj_size
    }

    pub fn alloc<P: PageProvider>(&mut self, provider: &mut P) -> Option<NonNull<u8>> {
        // Fast path: chercher un slab avec une place libre
        let mut cur = self.head;
        while let Some(hdr) = cur {
            // SAFETY:
	    // - `hdr` provient d’un SlabHeader initialisé par Slab::init()
	    // - header vit au début d’une page slab encore vivante
	    // - liste intrusive ne contient que des headers valides
            let mut slab = unsafe { Slab::from_hdr(hdr) };
            if let Some(p) = slab.alloc() {
                return Some(p);
            }
            cur = slab.next_hdr();
        }

        // Slow path: nouveau slab
        let page = provider.alloc_page()?;

        // SAFETY:
        // - `page` provient du provider => page valide, alignée, writable.
        // - obj_size/align cohérents pour ce cache.
        let mut new_slab = unsafe { Slab::init(page, self.obj_size, self.align)? };

        // Insérer en tête de liste
        unsafe {
            // # Safety
            // `new_slab` est valide et son header est dans la page.
            new_slab.set_next_hdr(self.head);
        }
        self.head = Some(new_slab.header_ptr());

        new_slab.alloc()
    }

    /// # Safety
    /// - `ptr` doit provenir d’un `alloc()` de CE cache (même size-class).
    /// - pas de double-free.
    pub unsafe fn dealloc(&mut self, ptr: NonNull<u8>) {
        let mut cur = self.head;

        while let Some(hdr) = cur {
            // SAFETY:
	    // - `hdr` provient d’un SlabHeader initialisé par Slab::init()
	    // - header vit au début d’une page slab encore vivante
	    // - liste intrusive ne contient que des headers valides
            let mut slab = unsafe { Slab::from_hdr(hdr) };

            if slab.contains(ptr) {
                // SAFETY:
                // - `ptr` appartient bien à ce slab (contains).
                // - pas de double free (précondition).
                slab.free(ptr);
                return;
            }

            cur = slab.next_hdr();
        }

        debug_assert!(false, "dealloc: ptr not found in cache slabs");
    }
}
