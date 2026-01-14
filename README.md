# Rust Slab Allocator (no_std)

Projet d’examen — Implémentation d’un allocateur de type **slab / slub minimal** en Rust **no_std**, accompagné d’un **write-up technique** sur l’allocateur **SLUB du kernel Linux**.

Projet réalisé en binôme dans le cadre du cours *Systèmes bas niveau / gestion mémoire*.

---

## 📌 Objectifs du projet

Ce projet a deux objectifs principaux :

1. **Comprendre le fonctionnement de l’allocateur SLUB du kernel Linux**
   - caches
   - slabs / pages
   - freelist intrusive
   - fast path / slow path
   - concurrence et hardening
   - lien avec les vulnérabilités liées aux allocateurs

2. **Implémenter un allocateur slab minimal en Rust no_std**
   - sans dépendre de `std`
   - avec une architecture simple, pédagogique et testable
   - en respectant strictement les contraintes de sécurité (`unsafe` documenté)

---

## 📅 Informations examen

- Projet réalisé en binôme (2 personnes maximum)
- Travail en continu sur Git (commits examinés)
- Projet **no_std**
- Tests unitaires obligatoires
- Documentation des blocs `unsafe` obligatoire
- Rendu sous forme de **git bundle** (avec le dossier `.git`)
- Deadline : **18 janvier 2026**

---

## 👥 Auteurs

- Dylan Klein  
- Lélian Dupont  

Voir le fichier `Authors.md`

---

## 📜 Licence

Ce projet est distribué sous licence MIT.  
Voir le fichier `LICENSE`

---

## 🙏 Crédits

Certaines idées d’architecture sont inspirées de la documentation du kernel Linux
et de ressources publiques sur les allocateurs slab.

---

## 📂 Structure du dépôt

allocator/
├── Cargo.toml        # Configuration du crate
├── Cargo.lock        # Verrouillage des dépendances
├── src/
│   ├── lib.rs        # Entrée principale de la crate
│   ├── allocator.rs # Router global alloc/dealloc
│   ├── cache.rs     # Cache par classe de taille (multi-slab)
│   ├── slab.rs      # Gestion d’un slab (1 page = N objets)
│   ├── freelist.rs  # Freelist intrusive
│   └── page_provider.rs # Fournisseur de pages (4096 bytes)
├── tests/
│   └── basic.rs     # Tests d’intégration
└── slub.md           # Write-up technique SLUB

---

## 🧠 Write-up SLUB

Le document `slub.md` contient un **write-up technique détaillé** expliquant :

- les concepts slab / slub
- le rôle des caches
- la structure des slabs et des pages
- la freelist intrusive
- le fast path et le slow path
- les mécanismes de hardening (poisoning, etc.)
- un **parallèle explicite** avec l’allocateur Rust implémenté dans ce dépôt

👉 **Commencer par lire `slub.md`** pour comprendre la philosophie du code.

---

## ⚙️ Fonctionnement de l’allocateur

### Classes de tailles supportées

Les allocations sont routées vers des caches de tailles fixes :

8, 16, 32, 64, 128, 256, 512, 1024, 2048 bytes

- Toute taille non supportée retourne `null`
- Un alignement supérieur à la taille de la classe est rejeté

---

### Architecture simplifiée

- **PageProvider**
  - fournit des pages de 4096 bytes
  - version `no_std` : pool statique
  - OOM géré proprement (`None`)

- **Cache**
  - un cache par classe de taille
  - contient une **liste intrusive de slabs**
  - fast path : freelist non vide
  - slow path : nouvelle page → nouveau slab

- **Slab**
  - 1 page = 1 slab
  - découpe en objets de taille fixe
  - freelist intrusive stockée dans les objets libres

---

## 🔒 Sécurité et `unsafe`

Ce projet utilise `unsafe` **uniquement lorsque nécessaire**.

### Politique de sécurité

- Chaque fonction `unsafe fn` contient une section rustdoc :
/// # Safety
- Chaque bloc `unsafe {}` est accompagné d’un commentaire expliquant :
- les invariants attendus
- la provenance et l’alignement des pointeurs
- l’absence de double free
- l’ownership des pages et des slabs

👉 L’objectif est de rendre **chaque `unsafe` justifiable et auditable**.

---

## 🧪 Tests

Les tests sont **obligatoires** et couvrent :

- allocation et libération simples
- réutilisation de la freelist
- allocations multiples dans une même classe
- tailles non supportées
- alignements invalides
- OOM simulé
- test de régression multi-slab (libération dans le bon slab)

### Lancer les tests

🏗️ Build
Le projet est no_std, mais les tests utilisent std.

```
cargo build
```

```
cargo test
```
