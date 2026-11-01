# Asset ledger

Every third-party file in the repository, recorded when it's imported. Prefer CC0. CC-BY is allowed with credits. Anything else needs the user's OK.

The files themselves live in `client/assets/`, because Godot can only load files inside its project (ADR 0016). Godot's `.import` files and the textures it extracts from a glTF (`*_texture.png`) are derived from the listed file and share its licence.

| File or folder | Source URL | Author | License | Added | Notes |
|---|---|---|---|---|---|
| `client/assets/kaykit/Knight.glb`, `Rogue.glb`, `LICENSE.txt` | https://github.com/KayKit-Game-Assets/KayKit-Character-Pack-Adventures-1.0 (commit `672074b`, `addons/kaykit_character_pack_adventures/Characters/gltf/`) | Kay Lousberg (kaylousberg.com) | CC0 1.0 | 2026-10-26 | KayKit Adventurers 1.0, rigged with 76 animations each; used by the art style test (docs/design/art.md). Credit is optional but given here. |
| `client/addons/terrain_3d/` (all but `bin/`'s other platforms), `LICENSE.txt` | https://github.com/TokisanGames/Terrain3D/releases/tag/v1.1.0-stable (`Terrain3D_v1.1.0-stable.zip`, SHA-256 `c564638eaded41adcb8a4234fb5fc982bb4cbf73769b76354f16d287cab2b6ca`) | Cory Petkovsek, Roope Palmroos and contributors | MIT | 2026-10-31 | Terrain3D v1.1.0, the terrain editor and renderer from M2 (ADR 0032; the user accepted MIT native code, 2026-10-31). Native code: only the Linux and Windows x86_64 libraries are committed in `bin/` (ADR 0038); `terrain.gdextension` lists every platform's library. The demo folder isn't included. Its C# bindings compile as generated code (`client/addons/.editorconfig`). |
| `client/shaders/terrain3d.gdshader` | Terrain3D v1.1.0 `addons/terrain_3d/extras/shaders/minimum.gdshader` (same zip) | Cory Petkovsek, Roope Palmroos and contributors | MIT | 2026-10-31 | Derived: the colour map added, lit like our tiles (ADR 0038). |
