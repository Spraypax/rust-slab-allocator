#![no_std]

#[cfg(test)]
extern crate std;

pub mod page_provider;
pub mod cache;
pub mod allocator;
pub mod freelist;
pub mod slab;

// Re-export des interfaces publiques (pratique pour les tests et l'usage)
pub use page_provider::PageProvider;
pub use cache::Cache;
pub use crate::allocator::SlabAllocator;

/// Taille d'une page (backend). Fixée pour le projet.
pub const PAGE_SIZE: usize = 4096;

/// Tailles de caches supportées (size classes)
pub const SIZE_CLASSES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

/// Petite fonction utilitaire: renvoie l'index de cache correspondant à une taille.
pub fn size_class_index(size: usize) -> Option<usize> {
    SIZE_CLASSES.iter().position(|&c| size <= c)
}

#[cfg(test)]
mod tests {
    use core::alloc::Layout;

    #[cfg(miri)]
    type Prov = crate::page_provider::TestPageProvider;
    #[cfg(not(miri))]
    type Prov = crate::page_provider::StaticPageProvider<64>;

    fn make_allocator() -> crate::allocator::SlabAllocator<Prov> {
        crate::allocator::SlabAllocator::new(Prov::new())
    }

    #[test]
    fn alloc_dealloc_basic() {
        let mut a = make_allocator();

        let layout = Layout::from_size_align(32, 8).unwrap();

        let p1 = a.alloc(layout);
        assert!(!p1.is_null());

        let p2 = a.alloc(layout);
        assert!(!p2.is_null());
        assert_ne!(p1, p2);

        unsafe { a.dealloc(p1, layout) };
        unsafe { a.dealloc(p2, layout) };

        // réallocation doit marcher
        let p3 = a.alloc(layout);
        assert!(!p3.is_null());
    }

    #[test]
    fn unsupported_size_returns_null() {
        let mut a = make_allocator();

        let layout = Layout::from_size_align(4096 * 2, 8).unwrap();
        let p = a.alloc(layout);
        assert!(p.is_null());
    }

    #[test]
    fn alignment_constraint_respected_by_routing() {
        let mut a = make_allocator();

        let layout = Layout::from_size_align(24, 64).unwrap();
        let p = a.alloc(layout);

        // On refuse car align > size class (allocateur minimal)
        assert!(p.is_null());
    }
}

#[cfg(test)]
mod provider_tests {
    use crate::page_provider::PAGE_SIZE;
    use crate::page_provider::PageProvider;

    // Sous Miri, on utilise TestPageProvider pour éviter les faux positifs
    // Stacked Borrows du provider statique.
    #[cfg(miri)]
    type Prov = crate::page_provider::TestPageProvider;
    #[cfg(not(miri))]
    type Prov = crate::page_provider::StaticPageProvider<64>;

    #[test]
    fn page_is_4096_aligned() {
        let mut p = Prov::new();
        let page = p.alloc_page().expect("alloc page");
        assert_eq!((page.as_ptr() as usize) % PAGE_SIZE, 0);
        p.dealloc_page(page);
    }
}
