# Shiny Counter - reprise du projet

État de reprise préparé le 27 octobre 2026 à partir du code, du README et de
l'historique Git. Version applicative conservée : 0.1.12.

## Fonctionnement et choix de design

- Application native Rust avec eframe/egui, sans serveur distant obligatoire ni
  navigateur embarqué. L'interface est sombre, en cartes, avec accent violet par
  défaut, accent système et couleur personnalisable par préréglage ; FR et EN.
- `src/app/` répartit l'état et les écrans : compteur, pipettes, historique,
  réglages, sélection d'image et dialogues. `src/theme.rs` définit le thème.
- La capture lit des pixels d'un écran ou d'une fenêtre. Une signature utilise
  3 à 8 pipettes et une tolérance RVB ; `counter.rs` incrémente si toutes
  correspondent et ne réarme qu'après différence de toutes les pipettes.
  Une image immobile ne doit donc jamais multiplier les rencontres.
- `capture_worker.rs` réalise la capture hors du rendu. `storage.rs` sauvegarde
  atomiquement les préréglages et sessions dans la configuration utilisateur.
  Sous Windows : `%APPDATA%\ShinyCounter\config.json`, hors de ce dépôt.
- `server.rs` fournit l'overlay HTTP local pour OBS, sur 127.0.0.1:7878 par
  défaut. Les tests utilisent des ports locaux temporaires.
- `update.rs` contrôle la provenance, la taille et le SHA-256 des mises à jour.
  `build.rs` génère les icônes nécessaires aux exécutables et paquets.
  `assets/screenshot.png`, référencé dans le README, reste utile.

## Réalisé pendant ce nettoyage

- Inventaire des sources, tests, actifs, sorties de compilation et branches.
  Aucun travail local préexistant n'était en attente dans ce dépôt.
- Intégration des 8 branches Dependabot : eframe 0.37.0, ico 0.6.0,
  sha2 0.11.1, ureq 3.4.2, winresource 0.1.34, download-artifact 8.0.2,
  upload-artifact 7.0.2 et install-action 2.87.31.
- Alignement d'egui sur 0.37.0 avec eframe, afin de conserver une seule famille
  de types et d'éviter le doublon 0.36/0.37. Les quatre identifiants globaux de
  dialogues utilisent `Id::unique`, successeur identique de l'API dépréciée
  `Id::new`, pour conserver le Clippy strict.
- Correction des 6 vulnérabilités trouvées par RustSec dans les dépendances TLS :
  rustls 0.23.46, rustls-webpki 0.103.16 et webpki-roots 1.0.10.
  Remplacement de clipboard-win 5.4.1, retiré du registre, par 5.4.3.
- Conservation des sources, tests comportementaux, manifeste, verrou,
  attribution et screenshot ; sorties Cargo reproductibles archivées hors dépôt.
  Historique complet et branches initiales sauvegardés dans un bundle Git externe.

## Vérification locale du 27 octobre

- `cargo fmt --all -- --check` réussi.
- `cargo clippy --all-targets --all-features --locked -- -D warnings` réussi.
- `cargo test --all-targets --all-features --locked` : 55 tests réussis
  (46 bibliothèque, 7 état de l'application, 2 parcours du compteur).
- `cargo audit` ne signale plus de vulnérabilité ; 3 avertissements de
  maintenance subsistent pour `zbus_names`, `zvariant` et `zvariant_derive`.
- Ces résultats établissent les contrôles de source sur Windows avec Rust
  1.98.1. Ils ne prouvent pas une interaction manuelle avec la capture, les
  dialogues ou OBS, ni les résultats Linux/macOS ou le minimum Rust 1.95.

## Publication et suite

- Le push direct a été refusé par les règles GitHub : PR obligatoire, historique
  linéaire et 4 contrôles requis (Linux, Windows, macOS, Rust minimum 1.95).
  Le travail consolidé est publié dans la [PR 24](https://github.com/DylanBricar/ShinyCounter/pull/24)
  avec fusion squash normale. Consulter son état actuel et les références Git
  avant de reprendre ; conserver les branches Dependabot jusqu'à confirmation
  de cette fusion et de l'équivalence des sources intégrées.
- La version n'a pas été incrémentée. Le tag distant `v0.1.12` existe déjà.
  Le workflow Release, déclenché par une modification de Cargo.toml sur main,
  constate ce tag et garde `should_release=false` : aucune nouvelle version
  applicative n'est publiée par cette synchronisation Git.
- Une compilation de validation ne remplace pas le binaire installé. Aucun
  processus utilisateur, réglage, service ou serveur n'a été redémarré.
- Reste à qualifier l'interface native avec eframe 0.37 sur les plateformes
  concernées, et la capture/overlay sur une vraie session de chasse.
- Reste à suivre les dépendances amont concernées par les 3 avertissements de
  maintenance. Ne pas supprimer une dépendance de capture/portail uniquement
  pour masquer un avertissement.
- La CI exacte doit être distinguée des résultats locaux ; les autres dépôts
  du même compte ont actuellement leurs jobs bloqués par la facturation/plafond.
  Vérifier le run du SHA publié avant de déclarer la matrice CI réussie.
- La PR 24 a été fusionnée après ses quatre contrôles verts, avec équivalence
  complète des arbres vérifiée ; les huit branches Dependabot ont été supprimées.
  La CI du main `6334b41` a ensuite passé Windows, macOS et Rust minimum, mais
  l'action RustSec a échoué en créant son compte rendu GitHub : son audit indiquait
  zéro vulnérabilité et les trois avertissements de maintenance déjà connus.
  Le correctif ajoute seulement `checks: write` au job qui publie ce résultat,
  en conservant `contents: read`, les quatre contrôles requis et l'audit intégral.
  Aucun droit d'écriture d'issues ni exception d'audit n'est ajouté. La documentation
  de l'action décrit ce droit dans sa section [Granular Permissions](https://github.com/rustsec/audit-check/blob/69366f33c96575abad1ee0dba8212993eecbe998/README.md#granular-permissions).

Sauvegardes et journaux :
`C:\Users\YourTech\Documents\Codex\cleanup-2026-10-27\extensions\ShinyCounter`.

Pour reprendre : lire ce fichier et le README, puis exécuter
`git status --short --branch`, `cargo fmt --all -- --check`,
`cargo clippy --all-targets --all-features --locked -- -D warnings`,
`cargo test --all-targets --all-features --locked` et `cargo audit`.
