//! Freelist intrusive minimale.
//!
//! Les objets libres stockent un pointeur vers le prochain objet libre
//! dans leurs premiers octets.

use core::ptr::NonNull;

/// Noeud stocké dans un objet libre.
///
/// Le champ `next` est écrit directement dans la mémoire de l'objet.
#[repr(C)]
pub struct FreeNode {
    next: Option<NonNull<FreeNode>>,
}

impl FreeNode {
    /// Initialise un noeud libre avec un pointeur `next`.
    ///
    /// # Safety
    ///
    /// - `ptr` doit pointer vers une zone mémoire valide et alignée
    ///   pouvant contenir un `FreeNode`.
    /// - La mémoire pointée doit être considérée comme libre
    ///   (aucune donnée valide ne doit y être conservée).
    unsafe fn write(ptr: NonNull<u8>, next: Option<NonNull<FreeNode>>) {
        let node = ptr.as_ptr() as *mut FreeNode;
        (*node).next = next;
    }

    /// Lit le champ `next` depuis un objet libre.
    ///
    /// # Safety
    ///
    /// - `ptr` doit pointer vers un objet précédemment initialisé
    ///   comme `FreeNode`.
    unsafe fn read(ptr: NonNull<u8>) -> Option<NonNull<FreeNode>> {
        let node = ptr.as_ptr() as *const FreeNode;
        (*node).next
    }
}

/// Freelist intrusive LIFO.
pub struct FreeList {
    head: Option<NonNull<FreeNode>>,
}

impl FreeList {
    /// Crée une freelist vide.
    pub const fn new() -> Self {
        Self { head: None }
    }

    /// Retourne vrai si la freelist est vide.
    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Ajoute un objet à la freelist.
    ///
    /// # Safety
    ///
    /// - `ptr` doit être aligné correctement pour `FreeNode`.
    /// - `ptr` doit pointer vers une zone mémoire libre
    ///   (pas de double free).
    /// - L'objet doit appartenir au slab correspondant.
    pub unsafe fn push(&mut self, ptr: NonNull<u8>) {
        let next = self.head;
        FreeNode::write(ptr, next);
        self.head = Some(ptr.cast());
    }

    /// Retire et retourne un objet libre.
    ///
    /// # Safety
    ///
    /// - Tous les pointeurs stockés dans la freelist doivent être valides.
    pub unsafe fn pop(&mut self) -> Option<NonNull<u8>> {
        let head = self.head?;
        let next = FreeNode::read(head.cast());
        self.head = next;
        Some(head.cast())
    }
}
