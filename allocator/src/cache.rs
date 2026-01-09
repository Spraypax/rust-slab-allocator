use core::ptr::NonNull;

use crate::page_provider::PageProvider;
use crate::slab::Slab;

pub struct Cache {
    obj_size: usize,
    align: usize,
    slab: Option<Slab>,
}

impl Cache {
    pub const fn new(obj_size: usize, align: usize) -> Self {
        Self {
            obj_size,
            align,
            slab: None,
        }
    }

    #[inline]
    pub fn obj_size(&self) -> usize {
        self.obj_size
    }

    pub fn alloc<P: PageProvider>(&mut self, provider: &mut P) -> Option<NonNull<u8>> {
        if let Some(s) = self.slab.as_mut() {
            if let Some(p) = s.alloc() {
                return Some(p);
            }
        }

        let page = provider.alloc_page()?;

        // # Safety
        // - `page` provient de `provider.alloc_page()` => page valide et alignée (contrat PageProvider).
        // - `obj_size` et `align` sont les paramètres du cache (size class fixe), utilisés de manière cohérente.
        // - Le slab découpe la page en chunks et stocke une freelist intrusive: la page doit être writable et
        //   rester vivante tant que le Slab est utilisé.
        let mut s = unsafe { Slab::init(page, self.obj_size, self.align)? };


        let p = s.alloc()?;
        self.slab = Some(s);
        Some(p)
    }

    /// # Safety
    /// - `ptr` doit provenir d’un `alloc()` de CE cache (même size-class).
    /// - pas de double-free.
    pub unsafe fn dealloc(&mut self, ptr: NonNull<u8>) {
        if let Some(s) = self.slab.as_mut() {
            s.free(ptr);
        } else {
            debug_assert!(false, "dealloc on empty cache");
        }
    }
}
