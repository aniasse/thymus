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

## API

| Endpoint | Méthode | Description |
|----------|---------|-------------|
| `/api/health` | GET | Health check |
| `/api/status` | GET | État du système (phase, machines, mutations) |
| `/api/events` | POST | Ingestion d'un batch d'événements |
| `/api/mutations` | GET | Liste des mutations actives |
| `/api/profiles` | GET | Profils ADN des machines |
| `/api/activate` | POST | Passer de la Phase Thymus au mode actif |

## Licence

AGPL-3.0
