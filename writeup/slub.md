# Write-up : l’allocateur SLUB du kernel Linux (et parallèle avec notre allocateur minimal)

## 0. Objectif du document

Ce document a pour objectif d’expliquer le fonctionnement de l’allocateur **SLUB** du kernel Linux.
L’accent est mis sur la compréhension des mécanismes internes de l’allocateur, en particulier ceux
qui sont pertinents du point de vue **sécurité et exploitation** (freelist, métadonnées, concurrence).

L’objectif n’est pas de décrire exhaustivement l’implémentation du kernel Linux, mais de fournir
un modèle mental clair permettant :
- de comprendre comment les objets sont alloués et recyclés,
- d’identifier où se trouvent les pointeurs et les états critiques,
- de comprendre quelles corruptions mémoire sont possibles,
- et comment les mécanismes de hardening tentent de les empêcher.

Ce document inclut également un **parallèle explicite** avec notre implémentation d’un
allocateur slab minimal en Rust `no_std`, afin de relier la théorie à une implémentation concrète.

## 1. Rappels : allocation mémoire kernel et pages

### 1.1 Pages, buddy allocator, kmalloc

Dans le kernel Linux, la mémoire physique est gérée à l’aide d’un allocateur de pages
(buddy allocator). La mémoire est découpée en **pages** (généralement 4 KiB) qui peuvent
être allouées par blocs de tailles puissances de deux (order).

Les allocations de grande taille utilisent directement cet allocateur de pages.
Cependant, pour les allocations de **petits objets**, cette approche pose plusieurs problèmes :
- fragmentation interne importante,
- coût élevé de l’allocation/libération,
- absence de réutilisation optimisée des objets.

L’interface `kmalloc` repose historiquement sur des allocateurs de type slab afin de
fournir des allocations rapides et adaptées aux petits objets kernel.

### 1.2 Pourquoi un allocateur d’objets (slab) est nécessaire

De nombreuses structures kernel ont :
- une taille fixe,
- une durée de vie courte,
- des cycles allocation/libération fréquents.

Un allocateur slab repose sur l’idée de **mettre en cache des objets pré-formatés**
afin d’éviter de redemander de la mémoire au système à chaque allocation.

Les avantages principaux sont :
- meilleures performances (moins de gestion globale),
- meilleure localité cache,
- réduction de la fragmentation,
- possibilité d’initialiser/détruire les objets de manière contrôlée.

### 1.3 Contraintes spécifiques au kernel

Un allocateur kernel doit fonctionner sous des contraintes strictes :
- forte concurrence (multi-cœurs),
- contextes non bloquants,
- exigences de performance très élevées,
- robustesse face aux corruptions mémoire.

SLUB est conçu pour répondre à ces contraintes tout en restant suffisamment simple
pour être performant.

## 2. Concepts slab / slub : le modèle objet-cache

### 2.1 Object caching

Le principe fondamental des allocateurs slab est le **caching d’objets**.
Au lieu d’allouer de la mémoire brute, l’allocateur fournit des objets issus
d’un cache spécialisé pour une taille donnée.

Chaque cache est associé à :
- une taille d’objet fixe,
- un alignement,
- éventuellement des fonctions de construction/destruction.

Lorsqu’un objet est libéré, il n’est pas rendu immédiatement au système,
mais replacé dans une liste d’objets libres afin d’être réutilisé rapidement.

### 2.2 Cache (kmem_cache)

Dans SLUB, un cache représente un type d’objet.
Il contient :
- la taille et l’alignement des objets,
- des paramètres de configuration (debug, hardening),
- des pointeurs vers des slabs partiellement ou totalement utilisés.

Le cache est l’unité logique principale de l’allocateur :
toute allocation passe par un cache donné.

### 2.3 Slab et pages

Un **slab** correspond généralement à une ou plusieurs pages mémoire
découpées en objets de taille identique.

Chaque slab contient :
- un ensemble d’objets allouables,
- des métadonnées permettant de suivre quels objets sont libres ou utilisés.

Dans SLUB, les métadonnées de slab sont majoritairement stockées dans
la structure `page` associée à la page mémoire.

### 2.4 États d’un slab

Conceptuellement, un slab peut être dans différents états :
- **full** : aucun objet libre,
- **partial** : certains objets libres,
- **free** : tous les objets sont libres.

Ces états permettent à l’allocateur de choisir rapidement un slab
approprié lors d’une allocation ou d’une libération.

## 3. Structures et données clés (SLUB)

Cette section décrit les structures principales utilisées par SLUB, avec un focus sur
les champs critiques du point de vue sécurité et exploitation.

### 3.1 kmem_cache : le cache

La structure `kmem_cache` représente un cache d’objets d’une taille donnée.
Elle contient notamment :
- la taille de l’objet,
- l’alignement,
- des flags de configuration (debug, hardening),
- des pointeurs vers des listes de slabs (partiellement utilisés, etc.).

D’un point de vue exploitation, le cache définit :
- le **type d’objet** manipulé,
- la **taille exacte** des allocations,
- les règles de recyclage des objets.

Un attaquant cherche souvent à influencer **dans quel cache** une allocation se produit,
afin de provoquer une confusion de type ou une réutilisation contrôlée.

### 3.2 Slab et métadonnées de page

Dans SLUB, un slab est généralement représenté par une ou plusieurs pages mémoire.
Les métadonnées associées au slab sont stockées dans la structure `page`.

Ces métadonnées incluent :
- un pointeur vers la freelist du slab,
- le nombre d’objets libres ou utilisés,
- l’état du slab (full, partial, free),
- des liens vers d’autres slabs du même cache.

Ces informations sont critiques : leur corruption peut mener à des allocations arbitraires
ou à une perte de contrôle du flux d’allocation.

### 3.3 Freelist intrusive

SLUB utilise une **freelist intrusive** :
lorsqu’un objet est libre, les premiers octets de l’objet sont utilisés pour stocker
un pointeur vers le prochain objet libre.

Cela implique que :
- la mémoire de l’objet libre contient un pointeur valide,
- toute corruption de l’objet libre peut affecter la freelist,
- les bugs de type UAF, double free ou overflow peuvent manipuler ce pointeur.

La freelist est donc une cible privilégiée pour les exploits kernel,
car elle contrôle directement quelle adresse sera retournée lors de la prochaine allocation.

## 4. Chemins d’allocation : fast path vs slow path

SLUB distingue deux chemins principaux pour l’allocation :
le **fast path**, optimisé pour les performances, et le **slow path**,
utilisé lorsque les ressources locales sont insuffisantes.

### 4.1 Fast path

Le fast path est utilisé lorsque :
- un objet libre est disponible dans le cache local (souvent per-cpu),
- aucune synchronisation globale lourde n’est nécessaire.

Dans ce cas, l’allocateur :
1. lit le pointeur de tête de la freelist,
2. met à jour la freelist pour pointer vers l’objet suivant,
3. retourne l’objet à l’appelant.

Ce chemin est extrêmement rapide, mais repose sur des hypothèses fortes :
- la freelist est cohérente,
- les pointeurs stockés sont valides.

Toute corruption de la freelist a donc un impact immédiat.

### 4.2 Slow path

Le slow path est emprunté lorsque :
- la freelist locale est vide,
- ou lorsqu’une synchronisation avec l’état global est nécessaire.

Dans ce cas, SLUB peut :
- récupérer un slab partiellement utilisé,
- allouer une nouvelle page via l’allocateur de pages,
- initialiser une nouvelle freelist,
- mettre à jour les structures globales du cache.

Le slow path est plus coûteux et implique davantage de vérifications et de synchronisation,
mais il reste vulnérable à la corruption des métadonnées de slab.

### 4.3 Implications sécurité

Du point de vue sécurité :
- le fast path est souvent la cible principale, car il effectue peu de vérifications,
- le slow path est plus robuste mais plus complexe.

Comprendre quel chemin est emprunté dans un scénario donné est essentiel
pour raisonner sur la fiabilité et la reproductibilité d’un exploit.

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
