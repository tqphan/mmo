# Asset ledger

Every third-party file in the repository, recorded when it's imported. Prefer CC0. CC-BY is allowed with credits. Anything else needs the user's OK.

The files themselves live in `client/assets/`, because Godot can only load files inside its project (ADR 0016). Godot's `.import` files and the textures it extracts from a glTF (`*_texture.png`) are derived from the listed file and share its licence.

| File or folder | Source URL | Author | License | Added | Notes |
|---|---|---|---|---|---|
| `client/assets/kaykit/Knight.glb`, `Rogue.glb`, `LICENSE.txt` | https://github.com/KayKit-Game-Assets/KayKit-Character-Pack-Adventures-1.0 (commit `672074b`, `addons/kaykit_character_pack_adventures/Characters/gltf/`) | Kay Lousberg (kaylousberg.com) | CC0 1.0 | 2026-10-26 | KayKit Adventurers 1.0, rigged with 76 animations each; used by the art style test (docs/design/art.md). Credit is optional but given here. |
