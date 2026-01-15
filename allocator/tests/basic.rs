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

#[test]
fn dealloc_goes_to_correct_slab() {
    let provider = StaticPageProvider::<N_PAGES>::new();
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
    // Si bug ancien: p0 est poussé dans la freelist du slab 2 et ressort immédiatement => base0.
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
    // Provider minuscule: 1 seule page disponible
    let provider = StaticPageProvider::<1>::new();
    let mut a = SlabAllocator::new(provider);

    // 2048 => 2 objets max par page de 4096 (donc avec 1 page: 2 alloc OK, puis OOM)
    let layout = Layout::from_size_align(2048, 8).unwrap();

    let p1 = a.alloc(layout);
    assert!(!p1.is_null());

    let p2 = a.alloc(layout);
    assert!(!p2.is_null());

    // 3e alloc => besoin d'une nouvelle page => OOM => null
    let p3 = a.alloc(layout);
    assert!(p3.is_null());

}