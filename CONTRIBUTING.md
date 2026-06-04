# Contribuer à Thymus

Merci de votre intérêt ! Ce document décrit le flux de travail d'ingénierie du projet.

## Flux de travail

1. **Ouvrir une issue** décrivant le bug ou la fonctionnalité (templates fournis).
2. **Créer une branche** depuis `main` :
   - `feat/<sujet>` pour une fonctionnalité
   - `fix/<sujet>` pour un correctif
   - `chore/<sujet>` pour la maintenance / l'outillage
3. **Développer** en gardant les commits petits et focalisés.
4. **Ouvrir une Pull Request** qui référence l'issue (`Closes #N`).
5. **Revue de code** : au moins une revue avant le merge.
6. **Merge** en *squash* une fois la CI verte et la PR approuvée.

`main` reste toujours déployable. On ne pousse pas directement dessus.

## Conventions de commit

Format [Conventional Commits](https://www.conventionalcommits.org/), en minuscules et concis :

```
feat: add prometheus metrics endpoint
fix: correct beacon cooldown window
chore: bump ci to windows-latest
```

## Standards de code

Avant d'ouvrir une PR, ces commandes doivent passer :

```bash
cargo fmt --all -- --check
cargo clippy --all-targets        # zéro warning (clippy pedantic activé)
cargo test --all
```

Règles :

- **`unsafe` interdit** dans notre code (`unsafe_code = "forbid"`).
- **Clippy pedantic** activé au niveau du workspace ; pas de nouveau warning.
- **Tests obligatoires** pour tout nouveau comportement de détection ou d'état.
- Le code Windows (ETW) doit passer le cross-check : `cargo clippy --target x86_64-pc-windows-msvc -p thymus-detection -p thymus-sensor`.

## Architecture

```
crates/common      types partagés (events, profils, mutations)
crates/detection   moteur immunitaire + détecteurs (sans dépendance plateforme)
crates/sensor      agents (Linux /proc, Windows ETW, capture passive)
crates/core        serveur, API, persistance, dashboard
```

## Licence

En contribuant, vous acceptez que votre code soit publié sous **AGPL-3.0**.
