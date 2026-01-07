use core::{
    mem,
    ptr::NonNull,
};

use crate::page_provider::{PageProvider, PAGE_SIZE};

/// Header stocké au début de chaque page.
/// 1 page == 1 slab (minimal).
#[repr(C)]
struct SlabHeader {
    next: Option<NonNull<SlabHeader>>,
    freelist: Option<NonNull<u8>>,
    inuse: u16,
    capacity: u16,
    obj_size: u16,
    _pad: u16, // alignement + place
}

impl SlabHeader {
    fn is_full(&self) -> bool {
        self.inuse == self.capacity
    }
    fn is_empty(&self) -> bool {
        self.inuse == 0
    }
}

/// Cache d'une size class (objets de taille fixe).
pub struct Cache {
    obj_size: usize,
    head: Option<NonNull<SlabHeader>>,
}

impl Cache {
    pub const fn new(obj_size: usize) -> Self {
        Self { obj_size, head: None }
    }

    pub fn obj_size(&self) -> usize {
        self.obj_size
    }

    /// Alloue un objet depuis cette cache.
    pub fn alloc(&mut self, provider: &mut impl PageProvider) -> Option<NonNull<u8>> {
        // Fast-ish path: trouver un slab avec freelist non vide.
        let mut cur = self.head;
        while let Some(mut slab) = cur {
            unsafe {
                // # Safety
                // slab pointe vers un SlabHeader valide au début d'une page.
                let hdr = slab.as_mut();
                if let Some(obj) = hdr.freelist {
                    hdr.freelist = freelist_pop(obj);
                    hdr.inuse = hdr.inuse.checked_add(1)?;
                    return Some(obj);
                }
                cur = hdr.next;
            }
        }

        // Slow path: créer un nouveau slab.
        let page = provider.alloc_page()?;
        let slab = unsafe { init_slab(page, self.obj_size)? };
        unsafe {
            // # Safety
            // slab est un header nouvellement initialisé ; insertion en tête.
            slab.as_mut().next = self.head;
        }
        self.head = Some(slab);

        // Maintenant freelist non vide.
        unsafe {
            // # Safety
            // slab est valide, init_slab garantit une freelist non vide si capacity>0.
            let hdr = slab.as_mut();
            let obj = hdr.freelist?;
            hdr.freelist = freelist_pop(obj);
            hdr.inuse = 1;
            Some(obj)
        }
    }

    /// Libère un objet vers cette cache.
    pub fn dealloc(&mut self, provider: &mut impl PageProvider, ptr: NonNull<u8>) {
        let slab = slab_from_obj(ptr);

        // On pousse dans la freelist du slab.
        unsafe {
            // # Safety
            // slab_from_obj calcule la page de ptr, qui provient d'une page de cette cache.
            let hdr = slab.as_ptr();

            // On doit muter le header => cast mut.
            let hdr = hdr.cast_mut();
            let hdr_ref = &mut *hdr;

            freelist_push(ptr, hdr_ref.freelist);
            hdr_ref.freelist = Some(ptr);
            hdr_ref.inuse = hdr_ref.inuse.saturating_sub(1);

            // Optionnel (mais propre): si slab vide, rendre la page au provider.
            if hdr_ref.is_empty() {
                self.remove_slab_and_free(provider, slab);
            }
        }
    }

    fn remove_slab_and_free(&mut self, provider: &mut impl PageProvider, slab: NonNull<SlabHeader>) {
        // Retirer slab de la liste chainée self.head (O(n), acceptable en minimal).
        let mut prev: Option<NonNull<SlabHeader>> = None;
        let mut cur = self.head;

        while let Some(node) = cur {
            if node == slab {
                unsafe {
                    // # Safety
                    // node est un SlabHeader valide dans la liste.
                    let next = node.as_ref().next;
                    match prev {
                        None => self.head = next,
                        Some(mut p) => p.as_mut().next = next,
                    }

                    // libérer la page entière
                    let page = NonNull::new_unchecked(page_base(node.as_ptr() as *mut u8));
                    provider.dealloc_page(page);
                }
                return;
            }

            unsafe {
                // # Safety
                // node valide
                prev = Some(node);
                cur = node.as_ref().next;
            }
        }
    }
}

/// --- Freelist intrusive helpers ---

fn freelist_pop(head: NonNull<u8>) -> Option<NonNull<u8>> {
    unsafe {
        // # Safety
        // head pointe vers un objet libre ; ses premiers octets contiennent un pointeur next.
        let next_ptr = head.as_ptr() as *const *mut u8;
        NonNull::new(*next_ptr)
    }
}

fn freelist_push(obj: NonNull<u8>, old_head: Option<NonNull<u8>>) {
    unsafe {
        // # Safety
        // obj est un objet libre: on écrit un pointeur next dans ses premiers octets.
        let slot = obj.as_ptr() as *mut *mut u8;
        *slot = old_head.map(|p| p.as_ptr()).unwrap_or(core::ptr::null_mut());
    }
}

/// --- Slab init / address math ---

unsafe fn init_slab(page: NonNull<u8>, obj_size: usize) -> Option<NonNull<SlabHeader>> {
    // # Safety
    // page est une page de PAGE_SIZE bytes, exclusive, alignée comme donnée par PageProvider.

    let base = page.as_ptr() as usize;

    // Placer le header au début de page
    let hdr_ptr = base as *mut SlabHeader;
    let hdr_size = mem::size_of::<SlabHeader>();

    // obj_size doit pouvoir contenir un pointeur pour freelist intrusive
    let obj_size = obj_size.max(mem::size_of::<*mut u8>());

    // zone objets après le header, alignée
    let mut start = base + hdr_size;
    start = align_up(start, obj_size);

    if start >= base + PAGE_SIZE {
        return None;
    }

    let available = (base + PAGE_SIZE) - start;
    let capacity = (available / obj_size).min(u16::MAX as usize) as u16;

    // init header
    *hdr_ptr = SlabHeader {
        next: None,
        freelist: None,
        inuse: 0,
        capacity,
        obj_size: obj_size as u16,
        _pad: 0,
    };

    // construire la freelist: chaque objet pointe vers le suivant
    let mut head: Option<NonNull<u8>> = None;
    for i in (0..capacity as usize).rev() {
        let obj_addr = (start + i * obj_size) as *mut u8;
        let obj = NonNull::new(obj_addr)?;
        freelist_push(obj, head);
        head = Some(obj);
    }

    (*hdr_ptr).freelist = head;

    NonNull::new(hdr_ptr)
}

fn slab_from_obj(obj: NonNull<u8>) -> NonNull<SlabHeader> {
    let base = page_base(obj.as_ptr());
    unsafe {
        // # Safety
        // base est le début de la page contenant obj, et on y place un SlabHeader.
        NonNull::new_unchecked(base as *mut SlabHeader)
    }
}

fn page_base(p: *mut u8) -> *mut u8 {
    let addr = p as usize;
    (addr & !(PAGE_SIZE - 1)) as *mut u8
}

fn align_up(x: usize, a: usize) -> usize {
    debug_assert!(a.is_power_of_two() || a > 0);
    (x + (a - 1)) & !(a - 1)
}
