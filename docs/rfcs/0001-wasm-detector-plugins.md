# RFC 0001 — Détecteurs en plugins WebAssembly (sandboxés)

| | |
|---|---|
| **Statut** | Brouillon (en revue) |
| **Issue** | [#9](https://github.com/aniasse/thymus/issues/9) |
| **Auteur** | Équipe Thymus |
| **Crates touchés** | `core` (chargement + exécution), `common` (types partagés exposés), nouveau `detection-plugin-sdk` |
| **Cible** | Aucune version figée — RFC de conception |

---

## 1. Problème

Aujourd'hui, toute la logique de détection est compilée en dur dans `crates/detection`
(couche innée + adaptative). Le point d'entrée est une fonction pure :

```rust
// crates/detection/src/lib.rs
pub fn analyze_network_event(&self, event: &NetworkEvent, profile: &MachineIdentity)
    -> Option<Mutation>
```

Conséquences :

- **Fermé à l'extension.** Ajouter une règle (threat intel, métier, spécifique client)
  impose de modifier le cœur, recompiler, redéployer.
- **Pas de mise à jour à chaud.** Le cycle de réaction à une nouvelle menace est celui
  d'un build complet.
- **Pas de partage.** Aucune façon de distribuer une détection isolée et de confiance.

La signature `analyze_network_event` étant déjà une **fonction pure** (entrées → sortie,
sans effet de bord), c'est le point d'extension naturel : on peut l'externaliser sans
toucher au modèle de données ni au scoring.

## 2. Objectifs / Non-objectifs

### Objectifs

1. Charger des détecteurs **tiers** sans recompiler le Core.
2. Les exécuter dans une **sandbox capability-based** : aucun accès disque/réseau/OS.
3. **Borner CPU et mémoire** : un plugin buggé ou hostile ne peut ni figer ni planter le Core.
4. **Déterminisme** → détections reproductibles et testables.
5. **Rechargement à chaud** d'un dossier de plugins.
6. **Langage-agnostique** (Rust, TinyGo, AssemblyScript, …).

### Non-objectifs

- Le **chemin de capture** (`/proc/net`, ETW, raw sockets) reste **natif** : il a besoin
  d'un accès OS brut, à l'opposé de WASI. Hors scope.
- Les détecteurs du **hot-path** (`scan`, `beacon`, `exfil`) **restent compilés** : ils
  tournent par paquet, là où le coût de la frontière FFI serait rédhibitoire.
- Pas de WASM dans le **navigateur** : contredit le principe « zéro build JS » du dashboard.
- Pas de **marketplace** dans ce RFC (mentionné comme suite possible, §11).

## 3. Architecture : deux étages

```
                 ┌─────────────────── Core ───────────────────┐
 Network event → │                                             │
                 │  Étage 1 — natif, hot-path (par paquet)      │
                 │    scan · beacon · exfil · lateral           │ → Mutation
                 │                                             │
                 │  Étage 2 — extensible, WASM (batché)         │
                 │    candidate filter → [event;N] → sandbox    │ → Mutation
                 │                         ├ plugin Rust        │
                 │                         ├ plugin TinyGo      │
                 │                         └ plugin AssemblyScript
                 └─────────────────────────────────────────────┘
```

- **Étage 1 (natif).** Inchangé. Débit maximal, latence par paquet. C'est la défense de base.
- **Étage 2 (WASM).** Couche d'extension par-dessus. Ne reçoit pas chaque paquet : un
  **filtre de candidats** (cf. §7) sélectionne les events « intéressants », qui sont
  **batchés** puis passés à la sandbox. On amortit ainsi le coût de franchissement FFI.

Le résultat des deux étages est fusionné par le `scoring` existant : un plugin WASM produit
une `Mutation` partielle (score + détails + dimensions) traitée comme une source de plus.

## 4. Interface WIT (Component Model)

On part directement du **Component Model** (et non des modules core nus). Justification :
les types du domaine (`record`, `option`, `variant`, `list`) se mappent un-pour-un sur nos
structures `common`, ce qui supprime l'ABI manuel (pas de sérialisation à la main à travers
des pointeurs linéaires). Le surcoût du Component Model est négligeable devant le batching.

```wit
package thymus:detector@0.1.0;

interface types {
  record network-event {
    timestamp-unix-ms: u64,
    source-ip: string,
    source-port: u16,
    dest-ip: string,
    dest-port: u16,
    protocol: protocol,
    bytes-sent: u64,
    bytes-recv: u64,
    process-name: string,
    process-user: string,
  }

  enum protocol { tcp, udp, icmp }

  record machine-identity {
    machine-id: string,
    hostname: string,
    os: string,
    listening-ports: list<u16>,
    role: machine-role,
    known-peers: list<peer>,
    active-hour-start: u8,
    active-hour-end: u8,
    profile-maturity: f64,     // 0..1 — < seuil ⇒ Phase Thymus
    observation-days: u32,
  }

  enum machine-role { workstation, server, infrastructure, unknown }

  record peer {
    peer-ip: string,
    ports: list<u16>,
    avg-daily-volume: u64,
    avg-daily-connections: f64,
  }

  enum dimension { technical, relational, temporal, volumetric }

  record detail {
    dimension: dimension,
    description: string,
    expected-value: string,
    observed-value: string,
    deviation-sigma: f64,
  }

  // Mutation partielle : le Core remplit id, machine-id, scoring final, statut.
  record finding {
    risk-score: f64,           // 0..1, contribution du plugin
    dimensions: list<dimension>,
    details: list<detail>,
  }
}

world detector {
  use types.{network-event, machine-identity, finding};

  // Capacités fournies par l'hôte — strictement limitées.
  import host: interface {
    log: func(msg: string);
    now-unix-ms: func() -> u64;   // horloge fournie ⇒ déterminisme contrôlé
  }

  // Métadonnées interrogées au chargement.
  export manifest: func() -> string;          // JSON : nom, version, abi, auteur

  // Analyse un lot d'events ; renvoie au plus un finding par event (index aligné).
  export analyze-batch: func(
    events: list<network-event>,
    profile: machine-identity
  ) -> list<option<finding>>;
}
```

Notes de conception :

- **`analyze-batch`** (et non `analyze` unitaire) est l'export principal : il rend le
  batching explicite dans le contrat et minimise les franchissements de frontière.
- Les `IpAddr` deviennent des `string` (lisible, langage-agnostique). Coût parsing côté
  plugin acceptable ; alternative `tuple<u8,...>` écartée pour la lisibilité.
- Le plugin ne reçoit **que** event + profil : pas de `sensor_id`, `pid`, ni chemin
  d'exécutable — surface minimale, conforme au principe « le plugin ne voit que ce qu'on
  lui donne ».
- `host.now-unix-ms` est fourni par l'hôte plutôt que via WASI clocks, pour garder la main
  sur le déterminisme (cf. §5).

## 5. Garanties de la sandbox

Runtime : **wasmtime** avec Component Model + WASI Preview 2 (0.2.x). Baseline visée :
**wasmtime ≥ 27** (la première série où Component Model et WASI 0.2 sont stables) ;
on suit la dernière stable au moment de l'implémentation.

| Garantie | Mécanisme |
|---|---|
| Pas d'I/O disque/réseau | Aucune capacité WASI montée. Le `world detector` n'importe que `host` (log + horloge). Pas de `wasi:filesystem`, `wasi:sockets`, `wasi:cli`. |
| Bornage CPU | **Epoch interruption** : un timer d'hôte incrémente l'epoch ; dépassement ⇒ trap. Préféré au fuel pur car borne le **temps mural** par lot (le fuel borne les instructions, plus dur à calibrer). Budget : §6. |
| Bornage mémoire | `StoreLimits` : plafond mémoire linéaire par instance (défaut 64 Mo), nb d'instances borné. |
| Déterminisme | Pas d'horloge wall-clock WASI, pas de RNG ambiant, NaN canonicalisés (`Config::cranelift_nan_canonicalization`). Seule source de temps = `host.now-unix-ms`, loggée pour rejouabilité. |
| Isolation des pannes | Un trap (OOM, epoch, panic) est capturé : le finding du lot est abandonné, le plugin marqué dégradé, le Core continue. Jamais de propagation. |
| Pas d'état entre lots | Instances **pooling allocator**, réinitialisées par lot (ou par fenêtre courte). Empêche un plugin d'accumuler de l'état caché → renforce le déterminisme. |

Hypothèse de menace : un plugin est **non fiable par défaut**. La sandbox doit tenir même
face à un module hostile, pas seulement bogué.

## 6. Budget de latence & batching

Question ouverte du fil (« budget de latence cible »). Proposition chiffrée à valider par
benchmark :

- **Cible** : surcoût amorti **< 50 µs / event** à l'étage 2 (l'étage 1 natif reste sub-µs).
- **Taille de lot** : 64–256 events. À 256, le coût fixe d'un appel de composant
  (instanciation depuis le pool + marshalling) se dilue sous le seuil ; en deçà de 64 il
  domine.
- **Cadence** : flush du lot au plus tard toutes les **100 ms**, ou dès que le lot est plein —
  pour que la détection reste quasi-temps-réel sans famine ni rafales.
- **Filtre de candidats** (avant batching) : ne soumettre à l'étage 2 que les events qui
  ne sont pas déjà tranchés par l'étage 1 et qui touchent au moins une dimension surveillée
  par un plugin chargé (chaque manifeste déclare les dimensions qui l'intéressent). Réduit
  le volume soumis à WASM d'un ordre de grandeur attendu.

Ces nombres sont des **points de départ instrumentés**, pas des constantes gravées : la PR
d'implémentation devra inclure un bench `criterion` mesurant le surcoût réel par taille de lot.

## 7. Cycle de vie & rechargement à chaud

```
démarrage ──▶ scan dossier plugins/ ──▶ vérif signature ──▶ lecture manifest
   │                                                              │
   │                                              compile + pré-instancie (pool)
   │                                                              │
   ▼                                                              ▼
 registre des détecteurs actifs ◀──────────────── prêt (dimensions déclarées)

à chaud : watcher FS (debounce) ──▶ nouveau/modifié ──▶ même pipeline ──▶ swap atomique
                                  └▶ supprimé ──▶ retrait du registre
```

- **Chargement** : dossier configurable (`--plugins-dir`, défaut `./plugins`). Chaque entrée
  = un composant `.wasm` signé.
- **Swap atomique** : un plugin rechargé remplace l'ancien via `arc-swap` sur le registre ;
  les lots en vol terminent sur l'ancienne instance, les suivants prennent la nouvelle.
- **Dégradation** : un plugin qui trap de façon répétée (seuil configurable) est
  **désactivé** automatiquement et signalé (log + futur compteur Prometheus
  `thymus_plugin_traps_total`), sans interrompre les autres.

## 8. Packaging, signature & confiance

Question ouverte du fil (« modèle de confiance »). Proposition :

- **Format** : un composant WASM (`name-version.wasm`) + manifeste embarqué (export
  `manifest`) déclarant `name`, `version` (semver), `abi` (version WIT, §9), `author`,
  `dimensions`.
- **Signature** : signature détachée **ed25519** (`name-version.wasm.sig`) sur le hash du
  composant. Léger, sans PKI lourde.
- **Modèle de confiance** : un ensemble de **clés publiques approuvées** configuré côté
  déploiement (`trusted_keys` dans la config Core). Un plugin dont la signature ne valide
  pas contre une clé approuvée est **refusé au chargement**. Drapeau de dev explicite
  `--allow-unsigned-plugins` (jamais par défaut, loggé bruyamment).
- **Évolutivité** : ce modèle « clés gérées par le déploiement » est compatible avec un
  futur registre central (clé du registre = simplement une clé approuvée de plus). Pas de
  dépendance à une autorité centrale dans le périmètre v1.

## 9. Versioning de l'interface

- L'interface WIT est versionnée en **semver** dans le nom de package (`thymus:detector@0.1.0`).
- Le Core déclare la/les **versions WIT supportées**. Au chargement, l'`abi` du manifeste est
  vérifié : incompatibilité majeure ⇒ refus avec message clair.
- Tant que le RFC est en `0.x`, l'interface peut casser entre mineures ; le passage en `1.0`
  gèle l'ABI et engage la rétrocompatibilité.

## 10. Plan d'implémentation par étapes

Pour dé-risquer, livrer incrémentalement (une PR par étape) :

1. **Spike** : crate `thymus-detection-plugin-sdk` + un composant Rust d'exemple ;
   charger et appeler `analyze-batch` depuis un binaire de test (pas encore dans le Core).
   Mesurer le surcoût FFI réel (bench) → valider/ajuster §6.
2. **Sandbox durcie** : epoch interruption, `StoreLimits`, pooling allocator, capture des
   traps. Tests : plugin qui boucle (epoch), qui sur-alloue (mémoire), qui panique (isolé).
3. **Intégration Core** : registre, filtre de candidats, batching, fusion dans le scoring.
   Drapeau `--plugins-dir`. Désactivé si dossier absent (zéro impact par défaut).
4. **Signature & confiance** (§8) + refus des non signés.
5. **Hot reload** (§7) + compteurs Prometheus.

Étapes 1–2 sont des prérequis durs ; 3 apporte la valeur ; 4–5 durcissent pour la prod.

## 11. Suite possible : « vaccins »

La mémoire immunitaire / les règles adaptatives apprises pourraient être empaquetées en
modules WASM **portables et signés** — une immunité exportable d'un déploiement à l'autre,
chargée dans la même sandbox. Réutilise toute la machinerie de ce RFC (sandbox, signature,
chargement). À spécifier dans un RFC dédié une fois l'étage 2 stabilisé.

## 12. Alternatives écartées

- **Modules core WASM nus (ABI manuel).** Plus légers à l'appel, mais imposent une
  sérialisation à la main des types du domaine à travers la mémoire linéaire — fragile et
  verbeux. Le Component Model l'élimine ; le batching efface l'écart de coût.
- **Lua / Rhai / moteur de règles embarqué.** Plus simple à embarquer, mais : pas de
  sandbox mémoire/CPU de niveau équivalent, mono-langage, et déterminisme moins garanti.
- **Plugins natifs (`dylib`/FFI).** Performance maximale mais **aucune** isolation : un
  plugin tiers tourne avec les droits du Core. Inacceptable pour du code non fiable.
- **Sous-processus + IPC.** Isolation OS réelle mais coût de franchissement et de
  sérialisation bien supérieur, et la sandbox dépend de la config OS plutôt que du runtime.

## 13. Questions résiduelles

- Faut-il exposer aussi `ProcessEvent` / `SystemEvent` aux plugins, ou se limiter au réseau
  pour la v1 ? (Proposition : réseau seul d'abord — c'est le flux le plus mûr.)
- Granularité du rechargement : par fichier (proposé) vs transaction sur tout le dossier ?
- Faut-il un cache de compilation (`Module::serialize`) sur disque pour accélérer le
  démarrage avec beaucoup de plugins ?
- Politique exacte de « dégradation » (combien de traps avant désactivation, fenêtre).

---

*RFC de conception — aucun engagement d'implémentation tant qu'elle n'est pas approuvée.*
