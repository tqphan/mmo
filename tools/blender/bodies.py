"""Body A and Body B, the characters' base meshes (docs/design/characters.md, ADR 0042).

Run with Blender 5.2:
    blender -b --python tools/blender/bodies.py -- [--mh DIR] [--render DIR] [--export DIR] [--used FILE]

The bodies start from MakeHuman's base mesh (hm08, CC0) and its shape targets (CC0), vendored in
tools/blender/makehuman/ (assets/LEDGER.md). This script is our own: it reads the mesh, blends the
targets the way MakeHuman's macros weight them (gender, age, muscle, weight, height, proportions,
breast size and firmness), adds our own choices on top, and converts to metres, z up, facing -y.
MakeHuman's code is not used. Eyes are our own spheres; skin, lips and underwear are vertex
colours. --used writes the target files read, one per line, for vendoring.
"""

import gzip
import math
import os
import sys

import bpy
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
MH = os.path.join(HERE, "makehuman")
used = []

# MakeHuman's macro sliders: each splits its range into parts, blending the label below with the
# label above (data/targets/macrodetails/macro.json). An empty label is no target.
MACROS = {
    "gender": [(-0.01, 1.01, "female", "male")],
    "age": [(-0.01, 0.1874998, "baby", "child"), (0.1874999, 0.49998, "child", "young"), (0.49999, 1.01, "young", "old")],
    "muscle": [(-0.01, 0.49998, "minmuscle", "averagemuscle"), (0.49999, 1.01, "averagemuscle", "maxmuscle")],
    "weight": [(-0.01, 0.49998, "minweight", "averageweight"), (0.49999, 1.01, "averageweight", "maxweight")],
    "proportions": [(-0.01, 0.4999, "uncommonproportions", ""), (0.5, 1.01, "", "idealproportions")],
    "height": [(-0.01, 0.49, "minheight", ""), (0.51, 1.01, "", "maxheight")],
    "cupsize": [(-0.01, 0.49998, "mincup", "averagecup"), (0.49999, 1.01, "averagecup", "maxcup")],
    "firmness": [(-0.01, 0.4998, "minfirmness", "averagefirmness"), (0.4999, 1.01, "averagefirmness", "maxfirmness")],
}


def labels(macro, value):
    """The labels a slider value blends, with their weights."""
    for lo, hi, low, high in MACROS[macro]:
        if lo <= value <= hi:
            t = (value - lo) / (hi - lo)
            return [(l, w) for l, w in ((low, 1 - t), (high, t)) if l and w > 0.01]
    return []


def combine(folder, prefix, macros, values):
    """Target files and weights for a combination of macros, as the products of their weights."""
    out = [("", 1.0)]
    for m in macros:
        out = [(f"{name}-{l}" if name else l, w * lw) for name, w in out for l, lw in labels(m, values[m])]
    return [(f"{folder}/{prefix}{name}.target.gz", w) for name, w in out if w > 0.01]


def macro_targets(values):
    """Every macro target for a set of slider values (MakeHuman's combinations)."""
    t = []
    for race, share in (("african", values["african"]), ("asian", values["asian"]), ("caucasian", values["caucasian"])):
        t += [(f, w * share) for f, w in combine("macrodetails", f"{race}-", ["gender", "age"], values)]
    t += combine("macrodetails", "universal-", ["gender", "age", "muscle", "weight"], values)
    t += combine("macrodetails/height", "", ["gender", "age", "muscle", "weight", "height"], values)
    t += combine("macrodetails/proportions", "", ["gender", "age", "muscle", "weight", "proportions"], values)
    if values["gender"] < 0.5:
        t += [(f, w * (1 - values["gender"])) for f, w in
              combine("breast", "", ["gender", "age", "muscle", "weight", "cupsize", "firmness"], values) if "/female-" in f]
    return t


def detail(name, value):
    """A region target such as ("head/head-square", 0.3); negative values use the -decr side."""
    if value < 0 and name.endswith("-incr"):
        return (name[:-5] + "-decr.target.gz", -value)
    return (name + ".target.gz", value)


BODIES = {
    "a": {
        "label": "Body A", "height": 1.85,
        "macros": {"gender": 1.0, "age": 0.5, "muscle": 0.95, "weight": 0.42, "height": 0.75, "proportions": 1.0,
                   "cupsize": 0.5, "firmness": 0.5, "african": 0.2, "asian": 0.2, "caucasian": 0.6},
        "details": [
            ("torso/torso-vshape-incr", 0.6), ("torso/torso-muscle-pectoral-incr", 0.4), ("torso/torso-muscle-dorsi-incr", 0.4),
            ("arms/l-upperarm-shoulder-muscle-incr", 0.5), ("arms/r-upperarm-shoulder-muscle-incr", 0.5),
            ("arms/l-upperarm-muscle-incr", 0.4), ("arms/r-upperarm-muscle-incr", 0.4),
            ("stomach/stomach-tone-incr", 0.7), ("neck/measure-neck-circ-incr", 0.4),
            ("head/head-square", 0.35), ("chin/chin-prominent-incr", 0.3), ("chin/chin-width-incr", 0.3),
            ("chin/chin-bones-incr", 0.3), ("forehead/forehead-temple-decr", 0.2),
            ("eyebrows/eyebrows-trans-down", 0.2), ("nose/nose-scale-vert-incr", 0.15),
            ("l-eye-scale", 0.15),
        ],
        "skin": (0.84, 0.60, 0.47), "lips": (0.70, 0.44, 0.39), "iris": (0.30, 0.20, 0.12), "brows": (0.40, 0.22, 0.12),
    },
    "b": {
        "label": "Body B", "height": 1.70,
        "macros": {"gender": 0.0, "age": 0.5, "muscle": 0.62, "weight": 0.38, "height": 0.45, "proportions": 1.0,
                   "cupsize": 0.55, "firmness": 0.75, "african": 0.15, "asian": 0.25, "caucasian": 0.6},
        "details": [
            ("torso/measure-waist-circ-decr", 0.35), ("hip/hip-scale-horiz-incr", 0.15),
            ("buttocks/buttocks-volume-incr", 0.3), ("stomach/stomach-tone-incr", 0.4),
            ("head/head-oval", 0.4), ("head/head-scale-horiz-decr", 0.05),
            ("chin/chin-width-decr", 0.25), ("nose/nose-scale-horiz-decr", 0.2), ("nose/nose-point-up", 0.3),
            ("mouth/mouth-lowerlip-volume-incr", 0.3), ("mouth/mouth-upperlip-volume-incr", 0.25),
            ("cheek/l-cheek-bones-incr", 0.3), ("cheek/r-cheek-bones-incr", 0.3),
            ("l-eye-scale", 0.35), ("neck/measure-neck-circ-decr", 0.2),
        ],
        "skin": (0.92, 0.71, 0.60), "lips": (0.78, 0.45, 0.45), "iris": (0.28, 0.36, 0.22), "brows": (0.42, 0.22, 0.13),
    },
}


def expand(details):
    out = []
    for name, value in details:
        if name == "l-eye-scale":
            out += [detail("eyes/l-eye-scale-incr", value), detail("eyes/r-eye-scale-incr", value)]
        else:
            out.append(detail(name, value))
    return out


def load_obj(path):
    """Vertices and the faces of each group of MakeHuman's base mesh."""
    verts, groups, group = [], {}, None
    with open(path) as f:
        for line in f:
            if line.startswith("v "):
                verts.append(Vector(tuple(float(x) for x in line.split()[1:4])))
            elif line.startswith("g "):
                group = line.split()[1]
            elif line.startswith("f "):
                groups.setdefault(group, []).append([int(x.split("/")[0]) - 1 for x in line.split()[1:]])
    return verts, groups


def apply_target(verts, rel, weight):
    path = os.path.join(MH, "targets", rel)
    if not os.path.exists(path):
        # MakeHuman has no file where a combination is the base mesh (an average cup with an
        # average firmness).
        print(f"no target {rel} (weight {weight:.2f}); skipped")
        return
    used.append(rel)
    with gzip.open(path, "rt") as f:
        for line in f:
            if line[0].isdigit():
                i, x, y, z = line.split()
                verts[int(i)] += Vector((float(x), float(y), float(z))) * weight


def build(key):
    spec = BODIES[key]
    base, groups = load_obj(os.path.join(MH, "base.obj"))
    verts = [v.copy() for v in base]
    for rel, w in macro_targets(spec["macros"]) + expand(spec["details"]):
        apply_target(verts, rel, w)
    # MakeHuman: decimetres, y up, facing +z. Ours: metres, z up, facing -y.
    pts = [Vector((v.x * 0.1, -v.z * 0.1, v.y * 0.1)) for v in verts]
    floor = min(pts[i].z for f in groups["body"] for i in f)
    top = max(pts[i].z for f in groups["body"] for i in f)
    scale = spec["height"] / (top - floor)
    pts = [(p - Vector((0, 0, floor))) * scale for p in pts]
    faces = groups["body"]
    used_idx = sorted({i for f in faces for i in f})
    remap = {old: new for new, old in enumerate(used_idx)}
    mesh = bpy.data.meshes.new(f"body_{key}")
    mesh.from_pydata([pts[i] for i in used_idx], [], [[remap[i] for i in f] for f in faces])
    mesh.validate()
    for p in mesh.polygons:
        p.use_smooth = True
    obj = bpy.data.objects.new(f"Body{key.upper()}", mesh)
    bpy.context.scene.collection.objects.link(obj)
    eyes = [eye(key, spec, [pts[i] for f in groups[g] for i in f], g[-5]) for g in ("helper-l-eye", "helper-r-eye")]
    height = max(p.z for p in mesh.vertices.values() for p in [p.co])
    paint(obj, spec, height, [sum((pts[i] for f in groups[g] for i in f), Vector()) / sum(len(f) for f in groups[g])
                               for g in ("helper-l-eye", "helper-r-eye")])
    return obj, eyes, height


def eye(key, spec, helper, side):
    """An eyeball where MakeHuman's eye helper sits: white, the iris and a dark pupil looking ahead."""
    centre = sum(helper, Vector()) / len(helper)
    radius = max((p - centre).length for p in helper) * 0.98
    bpy.ops.mesh.primitive_uv_sphere_add(segments=24, ring_count=16, radius=radius)
    o = bpy.context.active_object
    o.name = f"Eye{key.upper()}{side.upper()}"
    attr = o.data.color_attributes.new("Col", "BYTE_COLOR", "CORNER")
    for loop in o.data.loops:
        look = -o.data.vertices[loop.vertex_index].co.normalized().y
        c = (0.94, 0.93, 0.90) if look < 0.82 else spec["iris"] if look < 0.955 else (0.02, 0.02, 0.02)
        attr.data[loop.index].color = (*srgb_to_linear(c), 1.0)
    for p in o.data.polygons:
        p.use_smooth = True
    # Into the mesh itself, so the eye moves with its body's object.
    for v in o.data.vertices:
        v.co += centre
    return o


def paint(obj, spec, height, eye_centres):
    """Skin, brows, lips and underwear as vertex colours, from heights as a share of the body's."""
    mesh = obj.data
    attr = mesh.color_attributes.new("Col", "BYTE_COLOR", "CORNER")
    female = spec["macros"]["gender"] < 0.5
    cloth = Vector((0.64, 0.64, 0.62))
    for loop in mesh.loops:
        p = mesh.vertices[loop.vertex_index].co
        h = p.z / height
        c = Vector(spec["skin"])
        if female:
            if 0.465 < h < 0.555 and abs(p.x) < 0.2 and (h > 0.5 or abs(p.x) < 0.03 + (h - 0.465) * 3.2):
                c = cloth
            if 0.705 < h < 0.765 and abs(p.x) < 0.15 and (p.y < 0 or h > 0.745):
                c = cloth
        elif 0.43 < h < 0.555 and abs(p.x) < 0.22:
            c = cloth
        # Brows: an arch above each eye on the front of the face.
        for e in eye_centres:
            dx, dz = p.x - e.x, p.z - e.z
            arch = 0.019 - 0.25 * dx * dx / 0.02
            if p.y < e.y + 0.004 and abs(dx) < 0.024 and abs(dz - arch) < (0.0042 if female else 0.0058) * (1 - abs(dx) / 0.03):
                c = Vector(spec["brows"])
        # Lips, a little warmer than the skin round them.
        mouth = Vector(((eye_centres[0].x + eye_centres[1].x) / 2, 0, eye_centres[0].z - (0.075 if female else 0.08)))
        if p.y < eye_centres[0].y - 0.002 and abs(p.x) < 0.022 and abs(p.z - mouth.z) < 0.009:
            c = c.lerp(Vector(spec["lips"]), 0.6)
        attr.data[loop.index].color = (*srgb_to_linear(c), 1.0)


def srgb_to_linear(c):
    return tuple(((v + 0.055) / 1.055) ** 2.4 if v > 0.04045 else v / 12.92 for v in c)


def material(name, subsurface, rough):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    tree = mat.node_tree
    bsdf = tree.nodes["Principled BSDF"]
    col = tree.nodes.new("ShaderNodeVertexColor")
    col.layer_name = "Col"
    tree.links.new(col.outputs["Color"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Subsurface Weight"].default_value = subsurface
    bsdf.inputs["Subsurface Radius"].default_value = (0.01, 0.004, 0.002)
    return mat


def render(groups, heights, out):
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 1280
    scene.render.resolution_y = 720
    world = bpy.data.worlds.new("World")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.045, 0.045, 0.05, 1)
    scene.world = world
    floor = bpy.data.meshes.new("floor")
    floor.from_pydata([(-6, -6, 0), (6, -6, 0), (6, 6, 0), (-6, 6, 0)], [], [(0, 1, 2, 3)])
    fo = bpy.data.objects.new("Floor", floor)
    scene.collection.objects.link(fo)
    fm = bpy.data.materials.new("Floor")
    fm.use_nodes = True
    fm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.06, 0.056, 0.052, 1)
    floor.materials.append(fm)
    for name, energy, rot in (("Key", 3.5, (55, 0, -30)), ("Fill", 0.8, (65, 0, 120)), ("Rim", 3.0, (75, 0, 175))):
        light = bpy.data.lights.new(name, "SUN")
        light.energy = energy
        lo = bpy.data.objects.new(name, light)
        lo.rotation_euler = [math.radians(a) for a in rot]
        scene.collection.objects.link(lo)
    cam = bpy.data.cameras.new("Cam")
    co = bpy.data.objects.new("Cam", cam)
    scene.collection.objects.link(co)
    scene.camera = co

    def place(group, x, turn):
        for o in group:
            o.location = (x, 0, 0)
            o.rotation_euler = (0, 0, turn)

    backs = []
    for g in groups:
        b = []
        for o in g:
            c = o.copy()
            scene.collection.objects.link(c)
            b.append(c)
        backs.append(b)
    for g, b, (xf, xb) in zip(groups, backs, ((-1.5, -0.5), (0.5, 1.5))):
        place(g, xf, 0)
        place(b, xb, math.pi)
    cam.lens = 50
    co.location = (0, -7.2, 1.0)
    co.rotation_euler = (math.radians(90), 0, 0)
    scene.render.filepath = f"{out}/sheet.png"
    bpy.ops.render.render(write_still=True)
    for b in backs:
        for o in b:
            bpy.data.objects.remove(o)
    for i, g in enumerate(groups):
        for j, other in enumerate(groups):
            place(other, 0 if i == j else 5, math.radians(-20))
        cam.lens = 85
        co.location = (0, -1.1, heights[i] - 0.11)
        co.rotation_euler = (math.radians(90), 0, 0)
        scene.render.filepath = f"{out}/head_{'ab'[i]}.png"
        bpy.ops.render.render(write_still=True)


def main():
    global MH
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    opts = {argv[i]: argv[i + 1] for i in range(0, len(argv) - 1, 2)}
    MH = opts.get("--mh", MH)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    skin = material("Skin", 0.2, 0.5)
    eye_mat = material("Eye", 0.0, 0.15)
    groups, heights = [], []
    for key in ("a", "b"):
        body, eyes, height = build(key)
        body.data.materials.append(skin)
        for e in eyes:
            e.data.materials.append(eye_mat)
        print(f"BODY {key}: {len(body.data.polygons)} faces, {height:.3f} m")
        groups.append([body] + eyes)
        heights.append(height)
    if "--used" in opts:
        with open(opts["--used"], "w") as f:
            f.write("\n".join(sorted(set(used))) + "\n")
    if "--export" in opts:
        export(groups, opts["--export"])
    if "--render" in opts:
        render(groups, heights, opts["--render"])


def export(groups, out):
    """Each body with its eyes as one glTF binary, vertex colours included."""
    os.makedirs(out, exist_ok=True)
    for key, group in zip("ab", groups):
        bpy.ops.object.select_all(action="DESELECT")
        for o in group:
            o.select_set(True)
        bpy.context.view_layer.objects.active = group[0]
        bpy.ops.export_scene.gltf(filepath=os.path.join(out, f"body_{key}.glb"), use_selection=True,
                                  export_format="GLB", export_vertex_color="ACTIVE", export_yup=True)


main()
