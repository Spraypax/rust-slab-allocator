use core::ptr::NonNull;
use core::cell::UnsafeCell;

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

/// Une page de 4096 bytes alignée sur 4096.
#[repr(align(4096))]
#[derive(Copy, Clone)]
#[allow(dead_code)]
struct Page([u8; PAGE_SIZE]);

/// PageProvider no_std basé sur un pool statique de N pages.
///
/// - Allocation: pop sur une stack d'indices.
/// - Free: push sur la stack.
/// - OOM: None.
pub struct StaticPageProvider<const N: usize> {
    pool: [UnsafeCell<Page>; N],
    free_stack: [usize; N],
    free_len: usize,
}

impl<const N: usize> StaticPageProvider<N> {
    /// Crée un provider avec N pages disponibles.
    pub fn new() -> Self {
	    let mut free_stack = [0usize; N];
	    let mut i = 0;
	    while i < N {
		free_stack[i] = i;
		i += 1;
	    }

	    Self {
		pool: core::array::from_fn(|_| UnsafeCell::new(Page([0u8; PAGE_SIZE]))),
		free_stack,
		free_len: N,
	    }
	}

    fn page_ptr(&self, idx: usize) -> NonNull<u8> {
    // On évite `self.pool[idx]` (indexing => création d'une référence => retag Miri).
    let cell_ptr = core::ptr::addr_of!(self.pool[idx]); // *const UnsafeCell<Page>
    let page_ptr = unsafe { (*cell_ptr).get().cast::<u8>() }; // *mut u8

    // SAFETY: idx < N garanti par l'appelant, et la page dans le pool est non-null.
    unsafe { NonNull::new_unchecked(page_ptr) }
}

    fn index_from_ptr(&self, ptr: NonNull<u8>) -> Option<usize> {
	    let base = self.page_ptr(0).as_ptr() as usize;
	    let p = ptr.as_ptr() as usize;

	    let page_size = core::mem::size_of::<Page>(); // == PAGE_SIZE
	    let total = N * page_size;

	    if p < base || p >= base + total {
		return None;
	    }

	    let off = p - base;
	    if off % page_size != 0 {
		return None;
	    }

	    Some(off / page_size)
	}
}

impl<const N: usize> PageProvider for StaticPageProvider<N> {
    fn alloc_page(&mut self) -> Option<NonNull<u8>> {
        if self.free_len == 0 {
            return None;
        }

        self.free_len -= 1;
        let idx = self.free_stack[self.free_len];
        let page = self.page_ptr(idx);

        unsafe {
            // # Safety
            // - `page` pointe vers une région PAGE_SIZE valide dans `pool`.
            // - On écrit uniquement dans cette page.
            core::ptr::write_bytes(page.as_ptr(), 0, PAGE_SIZE);
        }

        Some(page)
    }

    fn dealloc_page(&mut self, ptr: NonNull<u8>) {
        let Some(idx) = self.index_from_ptr(ptr) else {
            debug_assert!(false, "dealloc_page: ptr not from this pool or misaligned");
            return;
        };

        if self.free_len >= N {
            debug_assert!(false, "dealloc_page: free stack overflow (double free?)");
            return;
        }

        self.free_stack[self.free_len] = idx;
        self.free_len += 1;
    }
}

#[cfg(test)]
mod static_provider_tests {
    use super::*;

    #[test]
    fn static_provider_alignment_and_oom() {
        let mut p = StaticPageProvider::<2>::new();

        let a = p.alloc_page().expect("page a");
        let b = p.alloc_page().expect("page b");

        assert_eq!((a.as_ptr() as usize) % PAGE_SIZE, 0);
        assert_eq!((b.as_ptr() as usize) % PAGE_SIZE, 0);

        assert!(p.alloc_page().is_none()); // OOM

        p.dealloc_page(a);
        let c = p.alloc_page().expect("page c");
        assert_eq!((c.as_ptr() as usize) % PAGE_SIZE, 0);
    }
}

#[cfg(all(test, not(miri)))]
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
    impl Drop for TestPageProvider {
	    fn drop(&mut self) {
		while let Some(p) = self.pages.pop() {
		    unsafe {
		        std::alloc::dealloc(p.as_ptr(), std::alloc::Layout::from_size_align_unchecked(PAGE_SIZE, PAGE_SIZE));
		    }
		}
	    }
	}
}
#[cfg(test)]
pub use self::test_provider::TestPageProvider;
