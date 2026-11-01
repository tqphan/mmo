# 0042: Character bodies start from MakeHuman's CC0 base mesh and targets, through our own script

- **Status:** Accepted, 2026-11-01 (user approved the MakeHuman route; Blender allowed). Amends ADR 0004 for characters.
- **Context:** The user wants characters that look like a commercial stylized pack: realistic bodies, stylized faces and hair. The first try sculpted bodies from metaballs in code; the bodies were passable, but the faces and hands weren't. MakeHuman's base mesh and targets are CC0 and anatomically sound, with good topology and an eye and face layout. Installing MPFB (GPL code) was blocked as outside code; its data is enough.
- **Decision:**
  - Bodies are built by `tools/blender/bodies.py` (ours) from MakeHuman's `base.obj` and the shape targets it needs, vendored in `tools/blender/makehuman/` (CC0, 4.4 MB) and listed in the ledger.
  - MakeHuman's and MPFB's code is not run or committed.
  - Two base bodies, A (male, 1.85 m) and B (female, 1.70 m); clothing and hair are fitted to them next.
- **Consequences:**
  - Rebuilding needs only Blender (5.2) and the repository.
  - MakeHuman's rig and weights (also CC0) are the natural next step for skinning.
  - Our characters share MakeHuman's topology, so anything made for it (clothes fitted by its helpers) can be adapted.
