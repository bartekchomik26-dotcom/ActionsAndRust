# Builds the `ans` content mod folder from the original jar's assets: the
# entity bundles, the geometry and animations they name, their textures
# (moved under textures/entity/, the only content mod textures Horizon packs
# into its atlas), and the sounds. usage: build_content.py <jar root> <out>
import json, os, re, shutil, sys, glob
src, out = sys.argv[1], sys.argv[2]
A = f"{src}/assets/ans"
ok = re.compile(r'^[a-z0-9_.\-/]+$')
copied, skipped, missing = 0, [], set()
def put(rel_src, rel_dst):
    global copied
    s = f"{src}/assets/{rel_src}"
    if not os.path.exists(s): missing.add(rel_src); return
    if not ok.match(rel_dst): skipped.append(rel_dst); return
    d = f"{out}/assets/{rel_dst}"; os.makedirs(os.path.dirname(d), exist_ok=True); shutil.copyfile(s, d); copied += 1
shutil.rmtree(out, ignore_errors=True)
geos, texs = set(), set()
for f in sorted(glob.glob(f"{A}/entity/*.json")):
    name = os.path.basename(f)
    put(f"ans/entity/{name}", f"ans/entity/{name}")
    d = json.load(open(f))
    if 'animation_file' in d:
        ns, p = d['animation_file'].split(':', 1)
        put(f"{ns}/geckolib/animations/{p}.animation.json", f"{ns}/geckolib/animations/{p}.animation.json")
    for g in d.get('geometry', {}).values(): geos.add(g)
    for t in d.get('textures', {}).values(): texs.add(t)
# Attachables (the jar's held-item models: 3D items, the first person arm's
# item poses): the index, each attachable, its animation file, geometry and textures.
put("ans/attachable/_index.json", "ans/attachable/_index.json")
for f in sorted(glob.glob(f"{A}/attachable/**/*.json", recursive=True)):
    rel = os.path.relpath(f, f"{src}/assets")
    if os.path.basename(f).startswith('_'): continue
    put(rel, rel)
    d = json.load(open(f))
    if 'animation_file' in d:
        ns, p = d['animation_file'].split(':', 1)
        put(f"{ns}/geckolib/animations/{p}.animation.json", f"{ns}/geckolib/animations/{p}.animation.json")
    for g in d.get('geometry', {}).values(): geos.add(g)
    for t in d.get('textures', {}).values(): texs.add(t)
for g in sorted(geos):
    ns, p = g.split(':', 1); put(f"{ns}/geckolib/models/{p}.geo.json", f"{ns}/geckolib/models/{p}.geo.json")
for t in sorted(texs):
    ns, p = t.split(':', 1)
    if ns == 'minecraft': continue          # vanilla's, from the game
    rest = p[len('textures/'):] if p.startswith('textures/') else p
    dst = rest if rest.startswith('entity/') else 'entity/' + rest
    put(f"{ns}/{p}", f"{ns}/textures/{dst}")
    meta = f"{src}/assets/{ns}/{p}.mcmeta"
    if os.path.exists(meta): put(f"{ns}/{p}.mcmeta", f"{ns}/textures/{dst}.mcmeta")
for fn in ['sounds.json', 'sound_meta.json', 'lang/en_us.json']:
    put(f"ans/{fn}", f"ans/{fn}")
for f in glob.glob(f"{A}/sounds/**/*.ogg", recursive=True):
    rel = os.path.relpath(f, f"{src}/assets"); put(rel, rel)
# `emissive_alpha` materials use the texture's alpha as how much it glows, not
# as transparency (a 3D blaze powder's pixels have alpha 0 or 1): ship the
# lit and glowing halves of each texture a render controller draws with one,
# listed in opaque_textures.json (the engine draws from them).
import re
from PIL import Image
opaque = set()
for f in glob.glob(f"{out}/assets/ans/attachable/**/*.json", recursive=True) + glob.glob(f"{out}/assets/ans/entity/**/*.json", recursive=True):
    try:
        d = json.load(open(f))
    except Exception:
        continue
    if not isinstance(d, dict):
        continue
    mats = d.get("materials") or {}
    em = {k for k, m in mats.items() if isinstance(m, dict) and m.get("render_type") in ("emissive_alpha", "emissive_alpha_cull", "emissive_opaque", "emissive_opaque_cull")}
    if not em:
        continue
    tex = d.get("textures") or {}
    for rc in d.get("render_controllers") or []:
        if not isinstance(rc, dict):
            continue
        # A controller without materials draws with the default one; its
        # textures may come from an array (`Array.x[...]`): take every
        # texture the controller names.
        used = set(re.findall(r"(?i)material\.([A-Za-z0-9_]+)", json.dumps(rc.get("materials")))) or {"default"}
        if used & em:
            for t in re.findall(r"(?i)texture\.([A-Za-z0-9_]+)", json.dumps(rc)):
                path = tex.get(t, "")
                if path.startswith("ans:textures/") and "glint" not in path:
                    opaque.add(path)
made = []
for t in sorted(opaque):
    rel = t.split("textures/", 1)[1]
    tex_path = f"{out}/assets/ans/textures/entity/{rel}"
    if not os.path.exists(tex_path):
        continue
    im = Image.open(tex_path).convert("RGBA")
    # The jar's split (`BedrockTextures`): the plain half (LIT / OPAQUE_LIT)
    # draws every texel with some alpha opaque and lit, the glow half (GLOW)
    # adds the colour at 255 - alpha, full bright. Here as two cut-out halves
    # that never overlap: `<name>__lit` holds the texels that mostly don't
    # glow (alpha 128 and up), `<name>__glow` those that do (1 to 127), which
    # the engine draws full bright. Only for a texture with some glow; the
    # rest is drawn lit from the texture itself.
    px = im.load()
    w, h = im.size
    if not any(0 < px[x, y][3] < 128 for y in range(h) for x in range(w)):
        continue
    lit = Image.new("RGBA", (w, h))
    glow = Image.new("RGBA", (w, h))
    lp, gp = lit.load(), glow.load()
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a >= 128:
                lp[x, y] = (r, g, b, 255)
            elif a > 0:
                gp[x, y] = (r, g, b, 255)
    lit.save(tex_path[:-4] + "__lit.png")
    glow.save(tex_path[:-4] + "__glow.png")
    made.append("ans:entity/" + rel[:-4])
json.dump(made, open(f"{out}/assets/ans/opaque_textures.json", "w"), indent=1)
print("glow splits:", len(made))

# The jar composes one texture at run time (`_atlas_sources.json`: dop.png's
# 16 px cells filled with vanilla block and item textures); the jar ships it
# empty. List its cells by the engine's sprite names: the engine points each
# quad in a cell at the vanilla sprite itself (`Host::atlas_cells`).
cells = {}
try:
    srcs = json.load(open(f"{src}/assets/ans/attachable/_atlas_sources.json"))
    for tex, entries in srcs.get("atlases", {}).items():
        rel = tex.split("textures/", 1)[1]
        w, h = Image.open(f"{out}/assets/ans/textures/entity/{rel}").size
        def vanilla(t):
            ns, path = t.split(":", 1)
            path = path.removeprefix("textures/").removesuffix(".png")
            return path if ns == "minecraft" else f"{ns}:{path}"
        cells["ans:entity/" + rel[:-4]] = {"w": w, "h": h, "cell": srcs.get("cell", 16), "cells": [[x, y, vanilla(t)] for x, y, t in entries]}
except FileNotFoundError:
    pass
json.dump(cells, open(f"{out}/assets/ans/atlas_cells.json", "w"))
# The textures the jar composes at load time (`BedrockTextures.composite`,
# taken from its classes into composites.json): the engine draws each part.
shutil.copy(os.path.join(os.path.dirname(os.path.abspath(__file__)), "composites.json"), f"{out}/assets/ans/composites.json")
print("composed atlases:", {k: len(v["cells"]) for k, v in cells.items()})

# Horizon's atlas takes textures up to 1024 wide and 512 tall: cut taller ones
# into overlapping 512-row parts `<name>__p<i>.png` (the engine points each quad at its
# part, `render::to_split_part`), and list them with their heights.
split = {}
for f in sorted(glob.glob(f"{out}/assets/*/textures/entity/**/*.png", recursive=True)):
    im = Image.open(f)
    w, h = im.size
    if h <= 512 or w > 1024:
        continue
    rel = os.path.relpath(f, f"{out}/assets")
    ns, path = rel.split("/textures/", 1)
    # Parts overlap (one every 256 rows, the last at the bottom) so a quad
    # near a cut lies wholly in one of them.
    n = -(-(h - 512) // 256) + 1
    for i in range(n):
        top = min(i * 256, h - 512)
        im.crop((0, top, w, top + 512)).save(f[:-4] + f"__p{i}.png")
    split[f"{ns}:{path[:-4]}"] = h
json.dump(split, open(f"{out}/assets/ans/split_textures.json", "w"), indent=1, sort_keys=True)
print("cut for the atlas:", len(split))
# The jar's settings screen look (`ans_ui`, from the 1.13 port3 jar): its
# Minecraft Ten font, icons and page titles (as GUI textures, `ans_ui:gui/...`)
# and its interface sounds.
ui = f"{src}/assets/ans_ui"
if os.path.isdir(ui):
    for rel in ["font/ten.json", "font/minecraft_ten.ttf", "font/minecraft_ten_copyright.txt", "sounds.json", "lang/en_us.json"]:
        if os.path.exists(f"{ui}/{rel}"):
            os.makedirs(os.path.dirname(f"{out}/assets/ans_ui/{rel}"), exist_ok=True)
            shutil.copyfile(f"{ui}/{rel}", f"{out}/assets/ans_ui/{rel}")
    for d in ["textures/gui", "sounds"]:
        if os.path.isdir(f"{ui}/{d}"):
            shutil.copytree(f"{ui}/{d}", f"{out}/assets/ans_ui/{d}", dirs_exist_ok=True)
    print("ui assets copied")
open(f"{out}/horizon-mod.toml", "w").write('''id = "ans"
name = "Actions & Beyond (assets)"
version = "1.0.0"
description = "Bedrock models, animations, textures and sounds for the Actions & Beyond native mod (ans_beyond). From Oreville Studios' Actions & Stuff, via the Actions & Beyond Fabric port."
authors = ["Oreville Studios (Actions & Stuff)", "ans-port contributors (Actions & Beyond)", "Horizon port"]
api = "0.2"
side = "both"
capabilities = ["registries"]
# The settings screen's font, pictures and sounds (the jar's `ans_ui`).
namespaces = ["ans_ui"]
''')
print("copied", copied, "skipped(bad names)", len(skipped), skipped[:5], "missing", len(missing), sorted(missing)[:8])
