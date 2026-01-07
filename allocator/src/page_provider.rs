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
pub mod test_provider {
    use super::*;
    use std::alloc::{alloc, dealloc, Layout};
    use std::vec::Vec;

    pub struct TestPageProvider {
        pages: Vec<NonNull<u8>>,
    }

    impl TestPageProvider {
        pub fn new() -> Self {
            Self { pages: Vec::new() }
        }
    }

    impl PageProvider for TestPageProvider {
        fn alloc_page(&mut self) -> Option<NonNull<u8>> {
            let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).ok()?;

            // SAFETY: layout valide, alloc renvoie un ptr aligné layout.align()
            let ptr = unsafe { alloc(layout) };
            let nn = NonNull::new(ptr)?;

            self.pages.push(nn);
            Some(nn)
        }

        fn dealloc_page(&mut self, ptr: NonNull<u8>) {
            let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE)
                .expect("layout must be valid");

            let idx = self.pages.iter().position(|&p| p == ptr)
                .expect("double free / unknown page");
            self.pages.swap_remove(idx);

            // SAFETY:
            // - ptr provient de alloc_page() avec le même Layout
            // - ptr n'a pas déjà été libéré (on le retire de pages)
            unsafe { dealloc(ptr.as_ptr(), layout) };
        }
    }
}
pub use test_provider::TestPageProvider;
