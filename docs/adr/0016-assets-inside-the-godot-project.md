# 0016: Third-party and generated assets live inside the Godot project

- **Status:** Accepted, 2026-10-26
- **Context:**
  - PLAN §4.8 puts assets in `assets/` at the repo root, next to `LEDGER.md`.
  - Godot can only load resources inside its project (`res://`). A symlink would work on Linux but not reliably on Windows (ADR 0008), and copying them in at build time would make two copies to keep in step.
- **Decision:**
  - Assets go in `client/assets/<source>/`, each folder with its source's licence file.
  - The ledger stays at `assets/LEDGER.md` and lists every file by its path under `client/assets/`.
  - The `.import` files Godot writes next to each asset are committed, so both machines import the same way.
  - `cargo xtask client` and `capture` run Godot's headless import before every run (about 2 s when nothing changed), so new assets are picked up without the editor.
- **Consequences:**
  - The ledger and the files it lists sit in different folders, so adding an asset means touching both.
  - Every client run costs about 2 s more than before. Before, the import only ran once, and new assets were never imported.
