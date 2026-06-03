# Thymos

**Le système immunitaire de votre réseau.**

Thymos est une plateforme de cybersécurité qui traite un réseau informatique comme un organisme vivant. Au lieu de chercher des attaques connues, Thymos construit l'identité comportementale du réseau (son "Soi") et détecte toute mutation anormale.

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
./target/release/thymos-core --listen 0.0.0.0:9443

# Lancer le Sensor (sur chaque machine)
./target/release/thymos-sensor --core-addr http://CORE_IP:9443

# Vérifier le statut
curl http://localhost:9443/api/status
```

## Dashboard

Le Core sert un dashboard web (HTMX, zéro build JS) directement sur le port d'écoute :

- `/` — État de l'organisme (phase, machines, mutations, maturité)
- `/mutations` — Mutations actives avec résolution
- `/machines` — ADN comportemental des machines
- `/network` — Cartographie réseau et chaînes latérales

## Authentification (optionnelle)

```bash
# Démarrer le Core avec un token
./target/release/thymos-core --token mon-secret

# Les sensors doivent fournir le token
./target/release/thymos-sensor --core-addr http://CORE_IP:9443 --token mon-secret
```

Sans `--token`, l'accès est ouvert. Avec un token : le dashboard exige une connexion
(page `/login`, session cookie) et l'API exige un header `Authorization: Bearer <token>`.

## Alerting webhook (optionnel)

```bash
./target/release/thymos-core --webhook https://hooks.example.com/thymos --webhook-min-score 0.7
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

## Licence

AGPL-3.0
