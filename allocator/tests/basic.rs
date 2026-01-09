use core::alloc::Layout;

use allocator::page_provider::StaticPageProvider;
use allocator::SlabAllocator;

const N_PAGES: usize = 64;

#[test]
fn alloc_free_reuse_same_sizeclass() {
    let provider = StaticPageProvider::<N_PAGES>::new();
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
    let provider = StaticPageProvider::<N_PAGES>::new();
    let mut a = SlabAllocator::new(provider);

    let layout = Layout::from_size_align(4096, 8).unwrap();
    let p = a.alloc(layout);
    assert!(p.is_null());
}

#[test]
fn alignment_too_large_returns_null() {
    let provider = StaticPageProvider::<N_PAGES>::new();
    let mut a = SlabAllocator::new(provider);

    let layout = Layout::from_size_align(32, 64).unwrap();
    let p = a.alloc(layout);
    assert!(p.is_null());
}

#[test]
fn alloc_multiple_then_free_all() {
    let provider = StaticPageProvider::<N_PAGES>::new();
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
