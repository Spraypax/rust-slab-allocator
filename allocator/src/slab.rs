//! Slab minimal : 1 page (4096) = 1 slab.
//!
//! Un slab est initialisé dans une page fournie par un `PageProvider`.
//! Le header est stocké au début de la page.

use core::{mem, ptr::NonNull};

use crate::freelist::FreeList;
use crate::page_provider::PAGE_SIZE;

/// Header stocké au début de chaque page.
/// Ce header vit DANS la page, pas d'allocation externe.
#[repr(C)]
pub struct SlabHeader {
    /// Lien vers le prochain slab du même cache (liste intrusive).
    next: Option<NonNull<SlabHeader>>,
    /// Freelist intrusive des objets libres dans cette page.
    freelist: FreeList,
    /// Nombre d'objets actuellement alloués.
    inuse: u16,
    /// Nombre total d'objets dans le slab.
    capacity: u16,
    /// Taille d'un objet (arrondie pour pouvoir stocker un pointeur de freelist).
    obj_size: u16,
    /// Alignement des objets (puissance de 2).
    align: u16,
}

/// Handle de slab : pointe sur le header au début de la page.
#[derive(Copy, Clone)]
pub struct Slab {
    hdr: NonNull<SlabHeader>,
}

impl Slab {
    /// Initialise un slab dans une page.
    ///
    /// - écrit le header dans les premiers octets de la page
    /// - découpe la zone restante en objets
    /// - remplit la freelist
    ///
    /// Retourne `None` si la page est trop petite (objets impossibles).
    ///
    /// # Safety
    /// - `page` doit pointer vers une page valide de PAGE_SIZE bytes.
    /// - `page` doit être alignée sur PAGE_SIZE (4096).
    /// - Cette page doit être exclusive à ce slab (pas partagée ailleurs).
    pub unsafe fn init(page: NonNull<u8>, obj_size: usize, align: usize) -> Option<Self> {
        // Validation minimale d'alignement
        if !align.is_power_of_two() || align > PAGE_SIZE {
            return None;
        }

        // obj_size doit permettre d'écrire un pointeur de freelist
        let min_obj = mem::size_of::<crate::freelist::FreeNode>();
        let obj_size = obj_size.max(min_obj);

        // Header au début de page
        let base_ptr = page.as_ptr(); // *mut u8
	let base = base_ptr as usize;
	let hdr_ptr = base_ptr.cast::<SlabHeader>();

        // Zone objets après le header
        let hdr_size = mem::size_of::<SlabHeader>();
        let mut start = base + hdr_size;

        // Aligner start sur `align`
        start = align_up(start, align);

        // Calcul capacity
        if start >= base + PAGE_SIZE {
            return None;
        }

        let available = (base + PAGE_SIZE) - start;
        let capacity = available / obj_size;
        if capacity == 0 {
            return None;
        }

        // Initialiser le header
        // SAFETY: hdr_ptr pointe dans la page, alignée au moins comme u8; repr(C) + align of SlabHeader.
        // Dans un vrai kernel, on garantirait l'alignement du header, ici on suppose page alignée 4096.
        core::ptr::write(
            hdr_ptr,
            SlabHeader {
            	next: None,
                freelist: FreeList::new(),
                inuse: 0,
                capacity: capacity.min(u16::MAX as usize) as u16,
                obj_size: obj_size.min(u16::MAX as usize) as u16,
                align: align.min(u16::MAX as usize) as u16,
            },
        );

        let mut slab = Slab {
            // SAFETY:
	    // - hdr_ptr pointe dans la page `page` fournie (PAGE_SIZE bytes)
	    // - hdr_ptr est non-null (page non-null)
	    // - la page est exclusive à ce slab pendant sa durée de vie
            hdr: NonNull::new_unchecked(hdr_ptr),
        };

	let start_off = start - base;
	
        // Remplir la freelist (LIFO) : push en reverse pour obtenir ordre croissant si on veut.        
	for i in (0..slab.capacity() as usize).rev() {
	    let off = start_off + i * obj_size;

	    // SAFETY:
	    // - off est dans la page (capacity calculée à partir de available/obj_size)
	    // - base_ptr est une page valide PAGE_SIZE bytes
	    let obj_addr = unsafe { base_ptr.add(off) };

	    let obj = NonNull::new(obj_addr)?;
	    // SAFETY: objet libre, on peut écrire le pointeur next dans l’objet
	    slab.hdr.as_mut().freelist.push(obj);
	}

        Some(slab)
    }

    	/// Alloue un objet depuis ce slab.
	pub fn alloc(&mut self) -> Option<NonNull<u8>> {
	    // SAFETY:
	    // - self.hdr pointe vers un SlabHeader valide dans une page vivante.
	    // - freelist contient des pointeurs initialisés par Slab::init().
	    unsafe {
		let hdr = self.hdr.as_mut();
		let ptr = hdr.freelist.pop()?;
		hdr.inuse = hdr.inuse.saturating_add(1);
		Some(ptr)
	    }
	}

    /// Libère un objet dans ce slab.
    ///
    /// # Safety
    /// - `ptr` doit appartenir à ce slab.
    /// - `ptr` ne doit pas être déjà libéré (pas de double free).
    pub unsafe fn free(&mut self, ptr: NonNull<u8>) {
	    // SAFETY:
	    // - self.hdr est un header valide.
	    // - ptr appartient à ce slab (précondition) et peut recevoir le next pointer de freelist.
	    let hdr = self.hdr.as_mut();
	    hdr.freelist.push(ptr);
	    hdr.inuse = hdr.inuse.saturating_sub(1);
	}

    pub fn capacity(&self) -> u16 {
    	// SAFETY: self.hdr pointe vers un SlabHeader écrit par Slab::init dans une page vivante
        unsafe { self.hdr.as_ref().capacity }
    }

    pub fn inuse(&self) -> u16 {
    	// SAFETY: self.hdr pointe vers un SlabHeader écrit par Slab::init dans une page vivante
        unsafe { self.hdr.as_ref().inuse }
    }

    pub fn is_empty(&self) -> bool {
        self.inuse() == 0
    }

    /// Vérifie si un pointeur est dans la page de ce slab.
    pub fn contains(&self, ptr: NonNull<u8>) -> bool {
        let base = self.page_base() as usize;
        let p = ptr.as_ptr() as usize;
        p >= base && p < base + PAGE_SIZE
    }

    /// Base de page (début du slab).
    pub fn page_base(&self) -> *mut u8 {
        self.hdr.as_ptr() as *mut u8
    }
    
    /// # Safety
    /// - `hdr` doit pointer vers un SlabHeader valide au début d'une page slab.
    pub unsafe fn from_hdr(hdr: NonNull<SlabHeader>) -> Self {
        Self { hdr }
    }

    pub fn header_ptr(&self) -> NonNull<SlabHeader> {
        self.hdr
    }

    pub fn next_hdr(&self) -> Option<NonNull<SlabHeader>> {
        unsafe { self.hdr.as_ref().next }
    }

    /// # Safety
    /// - `self` doit être un slab valide (header vivant dans la page).
    pub unsafe fn set_next_hdr(&mut self, next: Option<NonNull<SlabHeader>>) {
        self.hdr.as_mut().next = next;
    }
}

/// Arrondit `x` à l'alignement `a` (power-of-two).
fn align_up(x: usize, a: usize) -> usize {
    debug_assert!(a.is_power_of_two());
    (x + (a - 1)) & !(a - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page_provider::PageProvider;

    #[cfg(miri)]
    type Prov = crate::page_provider::StaticPageProvider<4>;
    #[cfg(not(miri))]
    type Prov = crate::page_provider::TestPageProvider;

    #[test]
    fn slab_init_alloc_free() {
        let mut prov = Prov::new();
        let page = prov.alloc_page().expect("page");

        let mut slab = unsafe { Slab::init(page, 32, 8).expect("slab init") };

        let a = slab.alloc().expect("alloc A");
        let b = slab.alloc().expect("alloc B");
        assert_ne!(a, b);

        unsafe {
            slab.free(a);
            slab.free(b);
        }

        assert!(slab.is_empty());

        prov.dealloc_page(page);
    }
}
