#![no_std]

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
