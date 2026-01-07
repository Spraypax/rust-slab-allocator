use core::ptr::NonNull;

/// Backend qui fournit des pages de 4096 bytes.
pub const PAGE_SIZE: usize = 4096;
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
#[cfg(test)]
pub struct TestPageProvider {
    pages: std::vec::Vec<NonNull<u8>>,
}

#[cfg(test)]
impl TestPageProvider {
    pub fn new() -> Self {
        Self { pages: std::vec::Vec::new() }
    }
}

#[cfg(test)]
impl PageProvider for TestPageProvider {
    fn alloc_page(&mut self) -> Option<NonNull<u8>> {
        // On alloue une page alignée via Vec<u8> puis on leak.
        let mut v = std::vec![0u8; PAGE_SIZE];
        let ptr = NonNull::new(v.as_mut_ptr())?;
        std::mem::forget(v);
        self.pages.push(ptr);
        Some(ptr)
    }

    fn dealloc_page(&mut self, ptr: NonNull<u8>) {
        // On retrouve la page et on la drop proprement.
        let idx = self.pages.iter().position(|&p| p == ptr)
            .expect("double free / unknown page");
        self.pages.swap_remove(idx);

        unsafe {
            // # Safety
            // ptr provient de alloc_page() ci-dessus, taille PAGE_SIZE.
            let _ = std::vec::Vec::from_raw_parts(ptr.as_ptr(), PAGE_SIZE, PAGE_SIZE);
        }
    }
}
