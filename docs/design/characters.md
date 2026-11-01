# Characters

Status: v0.1, 2026-11-01. The base meshes are done (Body A and Body B); clothing, hair, the rig, animations and the client's use of them come next. Code: `tools/blender/bodies.py`. Data: `tools/blender/makehuman/` (CC0). Output: `client/assets/characters/body_{a,b}.glb`. Decisions: ADR 0042.

## 1. How the original worked
| Fact | Confidence | Source |
|---|---|---|
| Player characters were semi-realistic, with a detailed character creator (face and body sliders) | A | public trailers and store pages |

Nothing of the original's art carries over (CLAUDE.md). The target look is the user's: realistic bodies with stylized faces and hair, soft painted skin and simple clothes. Their reference was a commercial pack on Fab, used only as a style target; nothing was taken from it.

## 2. Our rules

### Source
- The bodies start from **MakeHuman's base mesh (hm08) and shape targets**, CC0 since 2020. They come from the MPFB 2.0.18 repository's `data/` folder; the 65 targets used and `base.obj` are vendored in `tools/blender/makehuman/` with the licence, and listed in `assets/LEDGER.md`.
- **MakeHuman's code is not used.** MPFB's code is GPL and stays out of the repo. `bodies.py` is ours: it reads the OBJ and the gzipped targets (lines of `vertex dx dy dz`) and blends them the way MakeHuman's macros weight them.

### Building a body
`blender -b --python tools/blender/bodies.py -- [--render DIR] [--export DIR] [--used FILE]`
1. **Macros:** sliders for gender, age, muscle, weight, height, proportions, cup size and firmness, and race shares. Each slider splits into parts that blend the label below with the one above (`macro.json`). A combination's target weight is the product of its sliders' weights (race × gender × age; gender × age × muscle × weight [× height | × proportions | × cup × firmness]). Combinations with no file (an average cup with an average firmness) are the base mesh and are skipped.
2. **Details:** region targets on top (head shape, chin, nose, eyes, torso, arms).
3. **Conversion:** decimetres, y up, facing +z become metres, z up, facing −y. Only the `body` group is kept (13,378 quads, about 27k triangles). The body is moved onto z = 0 and scaled to its height.
4. **Eyes:** our own spheres where MakeHuman's eye helpers sit (white, iris, pupil as vertex colours).
5. **Paint:** vertex colours for skin, brows (an arch above each eye), lips and underwear (by height as a share of the body's).
6. **Export:** one `.glb` per body with its eyes, glTF y up.

### The two bodies
| | Body A | Body B |
|---|---|---|
| Height | 1.85 m | 1.70 m |
| Macros | male, 25, muscle 0.95, weight 0.42, ideal proportions | female, 25, muscle 0.62, weight 0.38, ideal proportions, cup 0.55, firmness 0.75 |
| Details | V-shaped torso, pecs, lats, shoulders and arms up; toned stomach; square head, strong chin and jaw | narrower waist, hips and glutes up; oval head, narrower chin and nose, fuller lips, cheekbones, larger eyes |
| Underwear | grey shorts | grey briefs and band |

## 3. Deviations and not yet done
- **Not in the game yet:** the client still draws capsules. Using the bodies needs the rig and animations.
- **No hair, clothes or rig yet.** Next, in order: clothing and hair (fitted to both bodies), the rig (MakeHuman's CC0 game-engine skeleton and weights are the plan), animations, then the client.
- **Faces are MakeHuman-realistic with small pushes.** The reference's stylization (larger eyes, sculpted hair) comes mostly with hair and further detail targets; tune after the user has seen them.
- **The pupils are vertex colours on a 24 × 16 sphere**, so their edge is jagged up close. A texture or a finer sphere will fix it.

## 4. Test plan
| What | How | Result, 2026-11-01 |
|---|---|---|
| Builds from the vendored data alone | `blender -b --python tools/blender/bodies.py -- --export client/assets/characters` | 13,378 faces each, 1.850 m and 1.700 m |
| Front and back, faces | `--render DIR` | `docs/captures/2026-11-01-bodies-{sheet,head-a,head-b}.png` |
| Godot imports them | the client's import (`cargo xtask capture …`) | no errors |
