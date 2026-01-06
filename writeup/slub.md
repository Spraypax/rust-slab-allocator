# Write-up : l’allocateur SLUB du kernel Linux (et parallèle avec notre allocateur minimal)

## 0. Objectif du document
Ce document explique le fonctionnement de l’allocateur **SLUB** du kernel Linux, avec un focus “compréhension + préparation exploitation” (où sont les pointeurs, quelles corruptions sont possibles, quelles protections existent).
Il inclut un parallèle explicite avec notre implémentation **slab allocator minimal** en Rust `no_std`.

## 1. Rappels : allocation mémoire kernel et pages
### 1.1 Pages, ordre (buddy), kmalloc vs slab
### 1.2 Pourquoi un allocateur d’objets (slab) existe
### 1.3 Contraintes kernel : perf, fragmentation, concurrence, debug/hardening

## 2. Concepts slab / slub : le modèle objet-cache
### 2.1 Object caching : réutilisation et localité
### 2.2 Cache (kmem_cache) : taille, alignement, constructeur/destructeur
### 2.3 Slab / page : découpage en objets
### 2.4 États : full / partial / free (intuition)

## 3. Structures et données clés (SLUB)
> But : identifier “où sont les pointeurs” et “où est l’état”
### 3.1 kmem_cache (le cache)
- Champs importants (taille objet, align, flags, etc.)
- Paramètres runtime (debug/hardening)

### 3.2 Page/slab metadata
- Comment une page représente un slab dans SLUB
- Où sont stockés : freelist, compteur d’objets, état slab

### 3.3 Freelist intrusive
- Principe : le next pointer est stocké dans l’objet libre
- Conséquences : UAF, double free, overflow → freelist corruption

## 4. Chemins d’allocation : fast path vs slow path
### 4.1 Fast path (per-cpu / allocation locale)
- Pourquoi per-cpu : éviter locks, latence
- Obtenir un objet depuis la freelist locale

### 4.2 Slow path (refill / nouvelle page / état partial)
- Refill depuis partial
- Allocation de nouvelles pages au besoin
- Retour d’objets au cache global

## 5. Libération : retour freelist + interactions per-cpu
### 5.1 Fast free (local)
### 5.2 Slow free (drain/partial/global)
### 5.3 Problèmes classiques : double free, UAF, type confusion

## 6. Concurrence et synchronisation (points importants)
### 6.1 Pourquoi SLUB est conçu pour réduire la contention
### 6.2 Per-cpu caches et “ownership” d’une freelist locale
### 6.3 Effets sur la reproductibilité d’un exploit

## 7. Hardening / sécurité (anti-exploitation)
> But : comprendre ce que l’exploit doit contourner
### 7.1 Freelist poisoning / encoding
### 7.2 Randomisation (ex: order/random freelist)
### 7.3 Checks d’intégrité et debug options (KASAN/KFENCE, etc.)
### 7.4 Impact concret sur UAF / double free / overflow

## 8. Parallèle avec notre allocateur minimal Rust `no_std`
> Section OBLIGATOIRE : faire correspondre “SLUB Linux” ↔ “notre code”
### 8.1 Ce que nous implémentons
- Caches de tailles fixes : 8..2048
- PageProvider 4096 bytes
- Freelist intrusive
- alloc(layout) / dealloc(ptr, layout)

### 8.2 Ce que nous n’implémentons PAS (et pourquoi)
- per-cpu caches
- états full/partial/free sophistiqués
- hardening avancé
- chemins lock-free

### 8.3 Tableau de correspondance
| Concept SLUB Linux | Rôle | Notre implémentation |
|---|---|---|
| kmem_cache | cache par type/taille | `Cache` |
| page/slab | backing store | `Slab` |
| freelist intrusive | objets libres | `FreeList` |
| per-cpu | fast path sans lock | (absent ou simplifié) |
| partial list | réservoir global | (simplifié) |

## 9. Mini “exploit mindset” (sans implémenter d’exploit)
### 9.1 Où frapper : freelist, metadata, recycles
### 9.2 Pourquoi slab allocators sont des cibles
### 9.3 Ce que SLUB hardening change

## 10. Références (liens et crédits)
- Docs / articles
- Code/kernel refs (si cités)
- Tout code externe utilisé dans le projet (si applicable)
