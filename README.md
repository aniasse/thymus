<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="media/thymus-mark-white.png">
    <source media="(prefers-color-scheme: light)" srcset="media/thymus-mark-dark.png">
    <img src="media/thymus-mark-white.png" alt="Thymus" width="120">
  </picture>
</p>

<h1 align="center">Thymus</h1>

<p align="center"><strong>Le système immunitaire de votre réseau.</strong></p>

Thymus est une plateforme de cybersécurité qui traite un réseau informatique comme un organisme vivant. Au lieu de chercher des attaques connues, Thymus construit l'identité comportementale du réseau (son "Soi") et détecte toute mutation anormale.

## Architecture

```
crates/
├── common/       # Types partagés : événements, profils ADN, mutations
├── detection/    # Moteur immunitaire : couche innée + adaptative + scoring
├── sensor/       # Agent déployé sur chaque machine (collecte + réponse locale)
└── core/         # Serveur central (analyse + cartographie + dashboard API)
```

## Concepts clés

- **ADN comportemental** : chaque machine possède un profil en 3 dimensions (technique, relationnel, temporel)
- **Phase Thymus** : période d'apprentissage où le système construit le modèle du Soi
- **Immunité innée** : règles universelles actives dès l'installation
- **Immunité adaptative** : règles spécifiques développées par l'apprentissage
- **Mémoire immunitaire** : chaque incident résolu accélère la réponse future

## Démarrage rapide

```bash
# Compiler
cargo build --release

# Lancer le Core
./target/release/thymus-core --listen 0.0.0.0:9443

# Lancer le Sensor (sur chaque machine)
./target/release/thymus-sensor --core-addr http://CORE_IP:9443

# Vérifier le statut
curl http://localhost:9443/api/status
```

## Deux modes de collecte

### Mode hôte (agent, par défaut)

L'agent est installé sur une machine et observe ses connexions via la facilité
native de la plateforme :

- **Linux** : lecture de `/proc/net` + enrichissement processus via `/proc/{pid}`
- **Windows** : ETW (Event Tracing for Windows), provider noyau TCP/IP — nécessite
  les droits Administrateur
- Autres plateformes : mode hôte non supporté (utiliser le mode passif)

```bash
./target/release/thymus-sensor --core-addr http://CORE_IP:9443
```

> L'agent Windows (ETW) est compilé et vérifié en CI (job `windows-latest`). Le
> test terrain sur un vrai poste Windows reste à valider.

### Mode passif (sans agent)

Un seul sensor branché sur un **port miroir (SPAN)** d'un switch capture les flux de
**tout le réseau** — y compris les appareils où aucun agent n'est installable :
imprimantes réseau, caméras IP, équipements IoT, automates, terminaux de paiement.

```bash
# Nécessite root / CAP_NET_RAW (Linux/macOS ; pas encore supporté sur Windows)
sudo ./target/release/thymus-sensor --interface eth0 --core-addr http://CORE_IP:9443
```

Le sensor passif n'analyse que les **métadonnées de flux** (qui parle à qui, quand,
combien) — jamais le contenu des paquets. Le Core profile chaque appareil local
(RFC1918) par son IP et construit l'ADN relationnel de l'écosystème complet.

Les appareils découverts passivement sont **étiquetés par résolution DNS inverse**
(PTR) quand le réseau le permet, et leur **type est inféré** depuis les ports servis
(imprimante, caméra IP, base de données, automate industriel, etc.).

## Dashboard

Le Core sert un dashboard web (HTMX, zéro build JS) directement sur le port d'écoute :

- `/` — État de l'organisme (phase, machines, mutations, maturité)
- `/mutations` — Mutations actives avec résolution
- `/machines` — ADN comportemental des machines
- `/network` — Cartographie réseau et chaînes latérales

## Authentification (optionnelle)

```bash
# Démarrer le Core avec un token
./target/release/thymus-core --token mon-secret

# Les sensors doivent fournir le token
./target/release/thymus-sensor --core-addr http://CORE_IP:9443 --token mon-secret
```

Sans `--token`, l'accès est ouvert. Avec un token : le dashboard exige une connexion
(page `/login`, session cookie) et l'API exige un header `Authorization: Bearer <token>`.

## Alerting webhook (optionnel)

```bash
./target/release/thymus-core --webhook https://hooks.example.com/thymus --webhook-min-score 0.7
```

Envoie un POST JSON à chaque mutation ou chaîne latérale dépassant le score minimum.

## API

| Endpoint | Méthode | Description |
|----------|---------|-------------|
| `/api/health` | GET | Health check (toujours ouvert) |
| `/api/status` | GET | État du système (phase, machines, mutations) |
| `/api/events` | POST | Ingestion d'un batch d'événements |
| `/api/mutations` | GET | Liste des mutations actives |
| `/api/mutations/{id}/resolve` | POST | Marquer une mutation comme résolue |
| `/api/mutations/{id}/false-positive` | POST | Marquer comme faux positif (crée une tolérance) |
| `/api/chains` | GET | Chaînes de déplacement latéral |
| `/api/profiles` | GET | Profils ADN des machines |
| `/api/tolerances` | GET | Entrées de tolérance immunitaire |
| `/api/context` | POST | Déclarer un contexte (maintenance, migration) |
| `/api/activate` | POST | Passer de la Phase Thymus au mode actif |
| `/api/login` | POST | Obtenir une session (si token configuré) |
| `/metrics` | GET | Métriques au format Prometheus (soumis au token) |

## Licence

AGPL-3.0
