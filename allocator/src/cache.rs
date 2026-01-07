use core::ptr::NonNull;

use crate::page_provider::{PageProvider, PAGE_SIZE};

/// Un cache gère une size class (ex: 64 bytes) + freelist + slabs.
/// Pour le commit 1 : uniquement les signatures stables.
pub trait Cache {
    /// Alloue un objet depuis ce cache.
    ///
    /// - Fast path: pop freelist
    /// - Slow path: demander une page au provider et initialiser une slab
    fn alloc(&mut self, provider: &mut impl PageProvider) -> Option<NonNull<u8>>;

    /// Libère un objet dans ce cache (push freelist)
    ///
    /// # Safety
    /// - `ptr` doit provenir d'une allocation effectuée par CE cache.
    /// - `ptr` ne doit pas être double-free.
    fn dealloc(&mut self, ptr: NonNull<u8>);
}

/// Impl "placeholder" pour compiler dès le début.
/// Vous remplacerez ça par un vrai type `SizeClassCache` plus tard.
pub struct DummyCache;

impl Cache for DummyCache {
    fn alloc(&mut self, _provider: &mut impl PageProvider) -> Option<NonNull<u8>> {
        None
    }

    fn dealloc(&mut self, _ptr: NonNull<u8>) {
        // no-op
    }
}
