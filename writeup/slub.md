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

## 5. Libération : retour des objets et recyclage

La libération d’un objet dans SLUB consiste à remettre l’objet dans la freelist
du cache correspondant, afin qu’il puisse être réutilisé ultérieurement.

### 5.1 Fast free

Dans le cas le plus courant, l’objet est libéré vers une freelist locale
(souvent per-cpu). Le processus est simple :
1. l’objet libéré est traité comme un objet libre,
2. un pointeur vers l’ancienne tête de freelist est écrit dans l’objet,
3. l’objet devient la nouvelle tête de la freelist.

Ce chemin est très rapide et implique peu de vérifications.

### 5.2 Slow free

Le slow free est utilisé lorsque :
- le cache local est saturé,
- ou lorsque le slab doit être déplacé entre différents états
(partial, free, etc.).

Dans ce cas, l’objet peut être rendu à une structure globale,
impliquant davantage de synchronisation et de gestion d’état.

### 5.3 Problèmes classiques liés à la libération

Les bugs liés à la libération sont parmi les plus dangereux :
- **double free** : insertion multiple du même objet dans la freelist,
- **use-after-free (UAF)** : écriture dans un objet déjà libéré,
- **type confusion** : réutilisation d’un objet pour un type différent.

Ces bugs permettent souvent de corrompre la freelist ou les métadonnées du slab,
ouvrant la voie à des allocations contrôlées.

## 6. Concurrence et synchronisation

SLUB est conçu pour fonctionner efficacement sur des systèmes multi-cœurs.
La concurrence est un facteur clé de sa conception.

### 6.1 Réduction de la contention

Pour éviter des verrous globaux coûteux, SLUB privilégie :
- des caches locaux (souvent per-cpu),
- des chemins d’exécution courts pour les opérations courantes.

Cela permet d’améliorer les performances, mais rend le comportement
plus complexe à analyser.

### 6.2 Effets sur la reproductibilité

Du point de vue exploitation :
- les freelists per-cpu rendent le comportement dépendant du CPU courant,
- les migrations de tâches peuvent changer le cache utilisé,
- l’état global du système influence fortement l’issue d’une allocation.

Ces facteurs rendent les exploits plus difficiles à rendre déterministes.

### 6.3 Simplification dans notre projet

Dans notre implémentation minimale, nous ne reproduisons pas cette complexité :
- pas de per-cpu caches,
- modèle de concurrence simplifié ou absent.

Cela permet de se concentrer sur les mécanismes fondamentaux
sans introduire de non-déterminisme inutile.

## 7. Hardening et protections

SLUB intègre plusieurs mécanismes visant à compliquer l’exploitation
des corruptions mémoire.

### 7.1 Freelist poisoning

Pour éviter la corruption directe de la freelist, SLUB peut :
- encoder les pointeurs de freelist (XOR, cookie),
- vérifier leur validité lors de l’allocation.

Cela empêche un attaquant de placer facilement une adresse arbitraire
dans la freelist.

### 7.2 Randomisation

SLUB peut introduire de la randomisation :
- ordre des objets dans un slab,
- emplacement des slabs en mémoire.

Ces mécanismes réduisent la prédictibilité du layout mémoire.

### 7.3 Vérifications et debug

En configuration debug, SLUB peut :
- détecter les double free,
- détecter certaines UAF,
- vérifier l’intégrité des métadonnées.

Des outils comme KASAN ou KFENCE renforcent ces protections,
au prix de performances.

### 7.4 Impact sur l’exploitation

Ces protections ne rendent pas les bugs impossibles,
mais augmentent fortement la complexité des exploits :
- nécessité de primitives plus puissantes,
- contournement des vérifications,
- dépendance à la configuration kernel.

## 8. Parallèle avec notre allocateur minimal Rust `no_std`

Notre projet implémente un allocateur slab minimal inspiré de SLUB,
dans un contexte volontairement simplifié.

## Mapping direct SLUB → notre code (parallèle concret)

Cette section fait le lien explicite entre les concepts SLUB et notre implémentation Rust no_std.

- **Fourniture de pages (4 KiB)** : `src/page_provider.rs`  
  - `PageProvider::{alloc_page, dealloc_page}`  
  - backend “pool statique” pour simuler un fournisseur de pages, avec OOM (`None`).

- **Freelist intrusive** : `src/freelist.rs`  
  - `FreeNode` stocké dans les objets libres  
  - `FreeList::{push, pop}` (LIFO)

- **Slab (1 page = 1 slab)** : `src/slab.rs`  
  - `Slab::init(page, obj_size, align)` : formatage de la page en objets alignés  
  - `Slab::{alloc, free, contains}`

- **Cache par size class** : `src/cache.rs`  
  - `Cache` gère une **liste intrusive de slabs** (équivalent simplifié d’une partial list)  
  - fast path : trouver un slab avec freelist non vide  
  - slow path : demander une nouvelle page au provider et créer un nouveau slab
  
### Note sur la gestion des slabs (différence vs SLUB)

SLUB maintient des structures plus riches (ex: listes partial/full, per-CPU caches, heuristiques) afin d’optimiser les allocations et limiter la contention.
Dans notre version minimaliste, chaque cache conserve une **liste intrusive de slabs** (une page = un slab) et parcourt cette liste pour trouver un slab avec un objet libre.  
Nous ne distinguons pas explicitement des états `partial/full/free` : c’est une simplification volontaire,suffisante pour illustrer le mécanisme de freelist et les invariants d’ownership.

- **Router (Layout → cache)** : `src/allocator.rs`  
  - sélection de la size class (8..2048) selon `Layout`  
  - rejet des tailles/alignements non supportés  
  - délégation à `Cache::alloc` / `Cache::dealloc`

### 8.1 Éléments implémentés

Notre allocateur reprend les concepts fondamentaux de SLUB :
- **caches d’objets de tailles fixes** (8 à 2048 bytes),
- **allocation par pages** via un PageProvider (4096 bytes),
- **découpage des pages en objets**,
- **freelist intrusive**, stockée directement dans les objets libres,
- **OOM allocateur (retour NULL)** : un test d’intégration vérifie que `SlabAllocator::alloc()` retourne `null` lorsque le `PageProvider` n’a plus de pages disponibles. L’objectif est de valider le comportement “out of memory” sans dépendre d’un nombre exact d’objets par page (qui varie selon `obj_size`, `align` et le header).
- API minimale `alloc(layout)` / `dealloc(ptr, layout)`.

Ces éléments permettent de reproduire le cycle de vie essentiel
des objets gérés par un allocateur slab.

### 8.2 Éléments volontairement absents

Afin de rester minimal et pédagogique, certaines fonctionnalités de SLUB
ne sont pas implémentées :
- caches per-cpu,
- gestion fine des états full/partial/free,
- mécanismes avancés de synchronisation,
- hardening (poisoning, randomisation).
- **Pas d’API globale `alloc/dealloc`** : l’allocateur est volontairement exposé uniquement via une instance `SlabAllocator`. On évite un état global (singleton) en `no_std` et on garde un modèle simple : l’appelant possède son allocateur et route explicitement les allocations via cette instance.


Ces absences sont assumées et documentées.

### 8.3 Correspondance conceptuelle

| SLUB Linux | Rôle | Notre implémentation |
|-----------|------|---------------------|
| kmem_cache | cache par type/taille | `Cache` |
| slab/page | backing store | `Slab` |
| freelist intrusive | objets libres | `FreeList` |
| per-cpu cache | fast path | non implémenté |
| partial list | réservoir global | simplifié |

Ce parallèle permet de relier directement les concepts théoriques
à une implémentation concrète.

### 8.4 Bonus — Validation mémoire avec Miri

Avant de parler de Miri, un point important côté implémentation : **chaque zone `unsafe` est documentée**. Chaque `unsafe fn` et chaque bloc `unsafe { ... }` possède une section `/// # Safety` (ou un commentaire Safety local) décrivant les invariants attendus : provenance des pointeurs, validité mémoire, alignement, absence de double free, et contraintes de lifetime. L’objectif est de rendre l’unsafe “audit-able” et cohérent avec les garanties minimales du projet.

Dans le cadre du bonus, notre implémentation de l’allocateur slab a été
validée à l’aide de l’outil Miri.

Miri est un interpréteur du langage Rust qui exécute le code en appliquant
un modèle mémoire strict, basé notamment sur Stacked Borrows.
Contrairement aux tests classiques, Miri permet de détecter dynamiquement
des comportements indéfinis (Undefined Behavior) liés à l’utilisation
de pointeurs bruts et de blocs unsafe.

Ce point est particulièrement pertinent dans le contexte d’un allocateur,
où l’implémentation repose volontairement sur :
- des pointeurs bruts (*mut u8),
- une freelist intrusive stockée dans la mémoire des objets libres,
- des manipulations manuelles d’ownership et d’aliasing,
- plusieurs invariants qui ne peuvent pas être exprimés directement par le type system.

#### Objectifs de la validation

L’objectif de l’exécution sous Miri était de vérifier l’absence de :
- violations d’aliasing entre références mutables,
- accès mémoire hors limites,
- use-after-free,
- double free,
- corruption de la freelist intrusive,
- fuites mémoire lors des cycles allocation / libération.

Autrement dit, il s’agissait de vérifier que le code unsafe respecte bien
les invariants implicites attendus par le modèle mémoire de Rust.

#### Adaptation de l’implémentation pour Miri

Afin de pouvoir exécuter les tests sous Miri tout en conservant un allocateur
no_std, un backend de fourniture de pages spécifique aux tests a été introduit.

Ce backend :
- repose sur std::alloc,
- est isolé derrière une feature dédiée (test-provider),
- n’est utilisé que pour les tests, jamais en production.

Cette séparation permet :
- de conserver une implémentation minimale et indépendante de std,
- tout en fournissant à Miri un environnement mémoire compatible avec son interprétation.

Les tests ont été exécutés avec la commande suivante :

```
cargo +nightly miri test --features test-provider
```

#### Résultats obtenus

L’ensemble de la suite de tests a été exécuté sous Miri avec succès :
- tests unitaires,
- tests d’intégration,
- scénarios multi-slabs,
- scénarios d’out-of-memory simulés.

## 9. Mini “exploit mindset”

Sans implémenter d’exploit, il est possible d’identifier
les zones critiques d’un allocateur slab.

### 9.1 Zones d’intérêt

Les cibles principales sont :
- la freelist intrusive,
- les métadonnées de slab/page,
- les mécanismes de recyclage des objets.

Ces zones contrôlent directement le comportement de l’allocateur.

### 9.2 Pourquoi les allocateurs slab sont ciblés

Les allocateurs slab sont attractifs pour l’exploitation car :
- ils manipulent de nombreux pointeurs,
- les objets sont réutilisés rapidement,
- les erreurs de type UAF ou double free ont des effets immédiats.

### 9.3 Impact du hardening

Le hardening ne supprime pas les bugs,
mais réduit les primitives exploitables :
- la corruption devient moins directe,
- les exploits sont plus complexes et dépendants du contexte.

Comprendre ces mécanismes est essentiel pour analyser
la faisabilité réelle d’un exploit.

## 10. Références

- Documentation du kernel Linux (mm, slab, slub)
- Code source du kernel Linux (mm/slub.c)
- Articles et présentations sur les allocateurs slab
- Ressources de formation sur l’exploitation kernel
