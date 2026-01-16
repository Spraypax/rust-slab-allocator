use core::alloc::Layout;

use allocator::SlabAllocator;

#[cfg(miri)]
use allocator::PageProvider;

#[cfg(not(miri))]
use allocator::page_provider::StaticPageProvider;

#[cfg(miri)]
use allocator::page_provider::TestPageProvider;

#[cfg(not(miri))]
const N_PAGES: usize = 64;

#[test]
fn alloc_free_reuse_same_sizeclass() {
    #[cfg(not(miri))]
    let provider = StaticPageProvider::<N_PAGES>::new();
    #[cfg(miri)]
    let provider = TestPageProvider::new();
    let mut a = SlabAllocator::new(provider);

    let layout = Layout::from_size_align(16, 8).unwrap();

    let p1 = a.alloc(layout);
    assert!(!p1.is_null());

    unsafe { a.dealloc(p1, layout) };

    let p2 = a.alloc(layout);
    assert!(!p2.is_null());

    assert_eq!(p2, p1);
}

#[test]
fn unsupported_size_returns_null() {
    #[cfg(not(miri))]
    let provider = StaticPageProvider::<N_PAGES>::new();
    #[cfg(miri)]
    let provider = TestPageProvider::new();
    let mut a = SlabAllocator::new(provider);

    let layout = Layout::from_size_align(4096, 8).unwrap();
    let p = a.alloc(layout);
    assert!(p.is_null());
}

#[test]
fn alignment_too_large_returns_null() {
    #[cfg(not(miri))]
    let provider = StaticPageProvider::<N_PAGES>::new();
    #[cfg(miri)]
    let provider = TestPageProvider::new();
    let mut a = SlabAllocator::new(provider);

    let layout = Layout::from_size_align(32, 64).unwrap();
    let p = a.alloc(layout);
    assert!(p.is_null());
}

#[test]
fn alloc_multiple_then_free_all() {
    #[cfg(not(miri))]
    let provider = StaticPageProvider::<N_PAGES>::new();
    #[cfg(miri)]
    let provider = TestPageProvider::new();
    let mut a = SlabAllocator::new(provider);

    let layout = Layout::from_size_align(64, 8).unwrap();

    let mut ptrs = [core::ptr::null_mut(); 32];

    for i in 0..ptrs.len() {
        let p = a.alloc(layout);
        assert!(!p.is_null());
        ptrs[i] = p;
    }

    for &p in &ptrs {
        unsafe { a.dealloc(p, layout) };
    }

    let p = a.alloc(layout);
    assert!(!p.is_null());
}

#[test]
fn dealloc_goes_to_correct_slab() {
    #[cfg(not(miri))]
    let provider = StaticPageProvider::<N_PAGES>::new();
    #[cfg(miri)]
    let provider = TestPageProvider::new();
    let mut a = SlabAllocator::new(provider);

    let layout = Layout::from_size_align(8, 8).unwrap();

    // 1) Premier alloc => slab 1
    let p0 = a.alloc(layout);
    assert!(!p0.is_null());
    let base0 = (p0 as usize) & !(4096 - 1);

    // 2) Allouer jusqu'à obtenir un ptr d'une autre page => slab 2
    let mut base1 = base0;
    let mut guard = 0usize;
    while base1 == base0 {
        let p = a.alloc(layout);
        assert!(!p.is_null());
        base1 = (p as usize) & !(4096 - 1);
        guard += 1;
        assert!(guard < 10000, "failed to reach second slab");
    }

    // 3) Free un ptr du slab 1
    unsafe { a.dealloc(p0, layout) };

    // 4) Prochaine alloc doit venir du slab HEAD (slab 2), pas retourner p0
    let p_next = a.alloc(layout);
    assert!(!p_next.is_null());
    let base_next = (p_next as usize) & !(4096 - 1);

    assert_eq!(
        base_next, base1,
        "allocation returned ptr from wrong slab (likely freelist corruption)"
    );
}

#[test]
fn allocator_oom_returns_null() {
    #[cfg(not(miri))]
    let provider = StaticPageProvider::<1>::new();
    #[cfg(miri)]
    let provider = LimitedProvider::new(1);
    let mut a = SlabAllocator::new(provider);

    let layout = Layout::from_size_align(2048, 8).unwrap();

    let mut got_one = false;
    for _ in 0..32 {
        let p = a.alloc(layout);
        if p.is_null() {
            break;
        }
        got_one = true;
    }

    assert!(got_one);

    let p_oom = a.alloc(layout);
    assert!(p_oom.is_null());
}

#[cfg(miri)]
struct LimitedProvider {
    inner: TestPageProvider,
    remaining: usize,
}

#[cfg(miri)]
impl LimitedProvider {
    fn new(limit: usize) -> Self {
        Self { inner: TestPageProvider::new(), remaining: limit }
    }
}

#[cfg(miri)]
impl PageProvider for LimitedProvider {
    fn alloc_page(&mut self) -> Option<core::ptr::NonNull<u8>> {
        if self.remaining == 0 {
            return None;
        }
        let p = self.inner.alloc_page()?;
        self.remaining -= 1;
        Some(p)
    }

    fn dealloc_page(&mut self, ptr: core::ptr::NonNull<u8>) {
        self.inner.dealloc_page(ptr);
        self.remaining += 1;
    }
}

