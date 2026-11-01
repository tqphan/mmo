# Asset ledger

Every third-party file in the repository, recorded when it's imported. Prefer CC0. CC-BY is allowed with credits. Anything else needs the user's OK.

The files themselves live in `client/assets/`, because Godot can only load files inside its project (ADR 0016). Godot's `.import` files and the textures it extracts from a glTF (`*_texture.png`) are derived from the listed file and share its licence.

| File or folder | Source URL | Author | License | Added | Notes |
|---|---|---|---|---|---|
| `client/assets/kaykit/Knight.glb`, `Rogue.glb`, `LICENSE.txt` | https://github.com/KayKit-Game-Assets/KayKit-Character-Pack-Adventures-1.0 (commit `672074b`, `addons/kaykit_character_pack_adventures/Characters/gltf/`) | Kay Lousberg (kaylousberg.com) | CC0 1.0 | 2026-10-26 | KayKit Adventurers 1.0, rigged with 76 animations each; used by the art style test (docs/design/art.md). Credit is optional but given here. |
| `tools/blender/makehuman/base.obj`, `targets/` (65 files), `LICENSE.md` | https://github.com/makehumancommunity/mpfb2 (tag `v2.0.18`, `src/mpfb/data/3dobjs/base.obj` and `src/mpfb/data/targets/`) | Data Collection AB, Joel Palmius, Jonas Hauquier (MakeHuman) | CC0 1.0 | 2026-11-01 | MakeHuman base mesh hm08 and the shape targets Body A and B use (docs/design/characters.md, ADR 0042). Only data; MPFB's GPL code is not used. `client/assets/characters/body_{a,b}.glb` are built from it by `tools/blender/bodies.py` and share its licence. |
