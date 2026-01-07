#![no_std]

#[cfg(test)]
extern crate std;

// Permet d'utiliser `core::alloc::Layout` etc.
use core::alloc::Layout;

pub mod page_provider;
pub mod cache;
pub mod allocator;

// Re-export des interfaces publiques (pratique pour les tests et l'usage)
pub use page_provider::PageProvider;
pub use cache::Cache;
pub use allocator::{alloc, dealloc};

/// Taille d'une page (backend). Fixée pour le projet.
pub const PAGE_SIZE: usize = 4096;

/// Tailles de caches supportées (size classes)
pub const SIZE_CLASSES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

/// Petite fonction utilitaire: renvoie l'index de cache correspondant à une taille.
pub fn size_class_index(size: usize) -> Option<usize> {
    SIZE_CLASSES.iter().position(|&c| size <= c)
}

/// API globale minimale (router).
/// Voir `allocator::alloc` / `allocator::dealloc`.
///
/// Note: l'impl concrète arrivera ensuite.
#[inline]
pub fn alloc_global(layout: Layout) -> *mut u8 {
    alloc(layout)
}

#[inline]
pub fn dealloc_global(ptr: *mut u8, layout: Layout) {
    dealloc(ptr, layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::alloc::Layout;
    use crate::page_provider::TestPageProvider;
    use crate::allocator::SlabAllocator;

    #[test]
    fn alloc_dealloc_basic() {
        let provider = TestPageProvider::new();
        let mut a = SlabAllocator::new(provider);

        let layout = Layout::from_size_align(32, 8).unwrap();

        let p1 = a.alloc(layout);
        assert!(!p1.is_null());

        let p2 = a.alloc(layout);
        assert!(!p2.is_null());
        assert_ne!(p1, p2);

        a.dealloc(p1, layout);
        a.dealloc(p2, layout);

        // réallocation doit marcher
        let p3 = a.alloc(layout);
        assert!(!p3.is_null());
    }

    #[test]
    fn unsupported_size_returns_null() {
        let provider = TestPageProvider::new();
        let mut a = SlabAllocator::new(provider);

        let layout = Layout::from_size_align(4096 * 2, 8).unwrap();
        let p = a.alloc(layout);
        assert!(p.is_null());
    }

    #[test]
    fn alignment_constraint_respected_by_routing() {
        let provider = TestPageProvider::new();
        let mut a = SlabAllocator::new(provider);

        let layout = Layout::from_size_align(24, 64).unwrap();
        let p = a.alloc(layout);
        assert!(!p.is_null());
        // on ne peut pas facilement vérifier l'align sans UB, mais on peut au moins tester modulo.
        assert_eq!((p as usize) % 64, 0);

        a.dealloc(p, layout);
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use crate::page_provider::PAGE_SIZE;
    use crate::page_provider::PageProvider;

    #[test]
    fn page_is_4096_aligned() {
        let mut p = crate::page_provider::TestPageProvider::new();
        let page = p.alloc_page().expect("alloc page");
        assert_eq!((page.as_ptr() as usize) % PAGE_SIZE, 0);
        p.dealloc_page(page);
    }
}
