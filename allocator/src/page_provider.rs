use core::ptr::NonNull;

/// Backend qui fournit des pages de 4096 bytes.
/// L'allocateur slab repose sur ce provider (slow path).
pub trait PageProvider {
    /// Alloue une page de taille PAGE_SIZE (4096) et renvoie un pointeur non nul.
    ///
    /// Retourne `None` en cas d'OOM.
    fn alloc_page(&mut self) -> Option<NonNull<u8>>;

    /// Libère une page précédemment allouée par `alloc_page`.
    ///
    /// # Safety
    /// - `ptr` doit provenir d'un `alloc_page` de CE provider.
    /// - `ptr` ne doit pas être déjà libéré.
    fn dealloc_page(&mut self, ptr: NonNull<u8>);
}
