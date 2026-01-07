use core::alloc::Layout;
use core::ptr::{self, NonNull};

use crate::size_class_index;
use crate::cache::{Cache, DummyCache};
use crate::page_provider::PageProvider;

pub struct DummyProvider;

impl PageProvider for DummyProvider {
    fn alloc_page(&mut self) -> Option<NonNull<u8>> {
        None
    }
    fn dealloc_page(&mut self, _ptr: NonNull<u8>) {}
}

pub fn alloc(layout: Layout) -> *mut u8 {
    let size = layout.size();
    let align = layout.align();

    if align > 2048 {
        return ptr::null_mut();
    }
    if size_class_index(size).is_none() {
        return ptr::null_mut();
    }

    let mut provider = DummyProvider;
    let mut cache = DummyCache;

    match cache.alloc(&mut provider) {
        Some(p) => p.as_ptr(),
        None => ptr::null_mut(),
    }
}

pub fn dealloc(ptr: *mut u8, layout: Layout) {
    if ptr.is_null() {
        return;
    }
    let size = layout.size();
    if size_class_index(size).is_none() {
        return;
    }

    let mut cache = DummyCache;

    unsafe {
        cache.dealloc(NonNull::new_unchecked(ptr));
    }
}
