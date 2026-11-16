// From a posed entity to quads: render controllers pick the geometry,
// texture, per-bone materials and visibility, and colours; the geometry's
// bones are posed (rest rotation plus the animation pose, GeckoLib's
// translate-pivot, rotate Z then Y then X, scale, translate back) and each
// cube's faces come out as quads in Horizon's mob frame.

use crate::bundle::{Bundle, Color, Material, RenderDef};
use crate::geo::{Face, Geometry, FACES};
use crate::molang::{Env, Scope, Value};
use crate::runtime::Pose;

use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [[f32; 4]; 4]);

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]);
    pub fn mul(&self, o: &Mat4) -> Mat4 {
        let mut r = [[0.0; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                r[i][j] = (0..4).map(|k| self.0[i][k] * o.0[k][j]).sum();
            }
        }
        Mat4(r)
    }
    pub fn translate(v: [f32; 3]) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.0[0][3] = v[0];
        m.0[1][3] = v[1];
        m.0[2][3] = v[2];
        m
    }
    pub fn scale(v: [f32; 3]) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.0[0][0] = v[0];
        m.0[1][1] = v[1];
        m.0[2][2] = v[2];
        m
    }
    fn rot(axis: usize, deg: f32) -> Mat4 {
        let (s, c) = deg.to_radians().sin_cos();
        let mut m = Mat4::IDENTITY;
        let (a, b) = match axis {
            0 => (1, 2),
            1 => (2, 0),
            _ => (0, 1),
        };
        m.0[a][a] = c;
        m.0[a][b] = -s;
        m.0[b][a] = s;
        m.0[b][b] = c;
        m
    }
    /// A Bedrock rotation (degrees) in Bedrock's model space: GeckoLib's
    /// `Rz(rz) Ry(-ry) Rx(-rx)` in its x-mirrored space, mirrored back.
    pub fn bedrock_rotation(r: [f32; 3]) -> Mat4 {
        Mat4::rot(2, -r[2]).mul(&Mat4::rot(1, r[1])).mul(&Mat4::rot(0, -r[0]))
    }
    pub fn point(&self, p: [f32; 3]) -> [f32; 3] {
        let m = &self.0;
        [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Opaque,
    Cutout,
    /// Glows: drawn full bright.
    Emissive,
    /// `emissive_alpha`: its texture's alpha says how much it glows, not
    /// where it is see-through. Drawn from the texture's lit and glowing
    /// halves (`<name>__lit`, `<name>__glow`, `opaque_textures.json`) when
    /// the pack has them, else from the texture, lit.
    EmissiveAlpha,
    /// Additive and translucent overlays, which Horizon's mob drawing can't
    /// blend: left out.
    Blended,
    Hidden,
}

pub fn kind_of(m: Option<&Material>) -> Kind {
    match m.map(|m| m.render_type.as_str()) {
        None | Some("cutout") | Some("cutout_cull") | Some("alphatest") => Kind::Cutout,
        Some("opaque") | Some("solid") => Kind::Opaque,
        // `emissive_opaque`: opaque, alpha is the glow, as `emissive_alpha`.
        Some("emissive_alpha") | Some("emissive_alpha_cull") | Some("emissive_opaque") | Some("emissive_opaque_cull") => Kind::EmissiveAlpha,
        Some("emissive") | Some("emissive_cutout") | Some("eyes") => Kind::Emissive,
        Some("invisible") | Some("none") => Kind::Hidden,
        Some("cutout_stencil_clip") => Kind::Cutout,
        // Plain see-through texture (a bucket's water, A&B's
        // `entity_alphablend`): Horizon's mob drawing can't blend, so it is
        // drawn as cutout rather than left out.
        // The pack's see-through items (glass, potions, the totem: its
        // `kipfdc`, a snowball's `ighgic`) likewise; the rest (enchanted
        // shells, the player's overlays) stays out.
        Some("translucent") | Some("translucent_cull") if m.is_some_and(|m| matches!(m.bedrock.as_str(), "entity_alphablend" | "kipfdc" | "ighgic")) => Kind::Cutout,
        Some(_) => Kind::Blended,
    }
}

/// One drawn layer: a render controller's geometry and texture.
#[derive(Clone, Debug)]
pub struct Layer {
    /// The render controller that drew it (`controller.render.*`).
    pub controller: String,
    pub geometry: String,
    pub texture: String,
    /// Per bone index: its kind (from the material rules).
    pub kinds: Vec<Kind>,
    /// Per bone index: its material samples more than one texture (not one of
    /// Bedrock's single-texture `entity*` materials), so the controller's
    /// further textures are drawn over it.
    pub multi: Vec<bool>,
    /// Per bone index: drawn or not (part_visibility, inherited).
    pub visible: Vec<bool>,
    /// 0xRRGGBB tint (color, overlay, hurt and fire colours folded in).
    pub tint: u32,
    pub full_bright: bool,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Quad {
    pub sprite: Rc<str>,
    /// Horizon order: top left, bottom left, bottom right, top right.
    pub corners: [[f32; 3]; 4],
    /// u0, v0, u1, v1 in texels of 16 per side of the sprite.
    pub uv: [f32; 4],
    pub tint: u32,
    pub full_bright: bool,
}

/// A namespaced texture path (`ans:textures/x/y.png`) as an atlas sprite.
/// Horizon packs a content mod's entity textures only, so the content mod
/// keeps A&B's textures under `textures/entity/`: `ans:entity/x/y`.
/// Vanilla's are named without their namespace (`entity/zombie/zombie`).
pub fn sprite_name(texture: &str) -> String {
    let (ns, path) = texture.split_once(':').unwrap_or(("minecraft", texture));
    let path = path.strip_prefix("textures/").unwrap_or(path);
    let path = path.strip_suffix(".png").unwrap_or(path);
    if ns == "minecraft" {
        path.to_string()
    } else if path.starts_with("entity/") {
        format!("{ns}:{path}")
    } else {
        format!("{ns}:entity/{path}")
    }
}

/// A texture the jar composes at load time from the game's own
/// (`BedrockTextures.composite`): a sheep's from its body, wool and undercoat,
/// whose wool parts take the wool colour.
pub struct Composite {
    /// Its size in pixels.
    pub size: [f32; 2],
    pub parts: Vec<CompositePart>,
}

pub struct CompositePart {
    /// The game's sprite it copies from.
    pub sprite: Rc<str>,
    /// The rectangle copied (`x, y, w, h` pixels), the whole sprite if none.
    pub src: Option<[f32; 4]>,
    /// Where it goes in the composite.
    pub dst: [f32; 2],
    /// Takes the mob's change colour (`entity_alphatest_change_color`).
    pub tinted: bool,
}

/// `q`, drawn on a composed texture, as one quad per part of it that holds
/// all of `q`'s texels, each on the game's sprite it comes from (pixel sizes
/// in `sizes`), later parts a hair in front of earlier ones as the jar paints
/// them over; a tinted part times `change` (0xRRGGBB, 0 none).
pub fn composite_parts(q: &Quad, c: &Composite, sizes: &crate::FastMap<String, [f32; 2]>, change: u32, out: &mut Vec<Quad>) {
    let px = |v: f32, n: f32| v / 16.0 * n;
    let (u0, v0, u1, v1) = (px(q.uv[0], c.size[0]), px(q.uv[1], c.size[1]), px(q.uv[2], c.size[0]), px(q.uv[3], c.size[1]));
    let (ulo, uhi, vlo, vhi) = (u0.min(u1), u0.max(u1), v0.min(v1), v0.max(v1));
    // The quad's facing, to put a part painted over another in front of it.
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let (e1, e2) = (sub(q.corners[1], q.corners[0]), sub(q.corners[2], q.corners[0]));
    let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
    let mul = |a: u32, b: u32| {
        let ch = |s: u32| ((((a >> s) & 0xFF) * ((b >> s) & 0xFF) + 127) / 255) << s;
        ch(16) | ch(8) | ch(0)
    };
    let mut k = 0.0;
    for p in &c.parts {
        let Some(&[sw, sh]) = sizes.get(&*p.sprite) else { continue };
        let [sx, sy, w, h] = p.src.unwrap_or([0.0, 0.0, sw, sh]);
        let [dx, dy] = p.dst;
        let e = 1e-3;
        if ulo < dx - e || uhi > dx + w + e || vlo < dy - e || vhi > dy + h + e {
            continue;
        }
        let mu = |u: f32| (u - dx + sx) / sw * 16.0;
        let mv = |v: f32| (v - dy + sy) / sh * 16.0;
        let off = k * 0.0006 / len;
        let corners = q.corners.map(|c| [c[0] + n[0] * off, c[1] + n[1] * off, c[2] + n[2] * off]);
        let tint = if p.tinted && change != 0 { mul(q.tint, change) } else { q.tint };
        out.push(Quad { sprite: p.sprite.clone(), corners, uv: [mu(u0), mv(v0), mu(u1), mv(v1)], tint, full_bright: q.full_bright });
        k += 1.0;
    }
    if k == 0.0 {
        // None of its parts known yet: the game's own sprite of that name.
        out.push(q.clone());
    }
}

/// A quad on the texture the jar composes at run time: the vanilla sprite in
/// the cell its texels lie in (`size` the texture's pixels, `cell` the cell
/// size), and its UV in that sprite's own 0..16. None outside every cell.
pub fn atlas_cell(uv: [f32; 4], size: [f32; 2], cell: f32, cells: &crate::FastMap<(u32, u32), Rc<str>>) -> Option<(Rc<str>, [f32; 4])> {
    let px = |v: f32, n: f32| v / 16.0 * n;
    let (x, y) = (px(uv[0].min(uv[2]), size[0]), px(uv[1].min(uv[3]), size[1]));
    let (cx, cy) = ((x / cell + 1e-3).floor() * cell, (y / cell + 1e-3).floor() * cell);
    let sprite = cells.get(&(cx as u32, cy as u32))?.clone();
    let local = |v: f32, n: f32, c: f32| (px(v, n) - c) / cell * 16.0;
    Some((sprite, [local(uv[0], size[0], cx), local(uv[1], size[1], cy), local(uv[2], size[0], cx), local(uv[3], size[1], cy)]))
}

/// The suffixes of a glow texture's halves (see [`Kind::EmissiveAlpha`]):
/// its texels that are lit, and those that glow (drawn full bright).
pub const LIT: &str = "__lit";
pub const GLOW: &str = "__glow";

/// Rows per part of a texture cut for the game's atlas (it takes 512 at most).
pub const SPLIT_ROWS: f32 = 512.0;
/// Rows between the tops of a cut texture's parts: they overlap, so a quad
/// near a cut lies wholly in one of them.
pub const SPLIT_STRIDE: f32 = 256.0;

/// The first row of part `i` of a texture `height` rows tall.
fn split_top(i: u32, height: f32) -> f32 {
    (i as f32 * SPLIT_STRIDE).min(height - SPLIT_ROWS)
}

/// Points `q` at the part of a cut texture (`height` pixels tall, parts of
/// [`SPLIT_ROWS`] every [`SPLIT_STRIDE`] rows, named `<sprite>__p<i>`) its
/// texels lie in, its v in that part's own 0..16.
pub fn to_split_part(q: &mut Quad, height: f32) {
    let px = |v: f32| v / 16.0 * height;
    let (top, bottom) = (px(q.uv[1].min(q.uv[3])), px(q.uv[1].max(q.uv[3])));
    let parts = (((height - SPLIT_ROWS) / SPLIT_STRIDE).ceil().max(0.0) + 1.0) as u32;
    let fits = |i: u32| {
        let s = split_top(i, height);
        s <= top + 1e-3 && bottom <= s + SPLIT_ROWS + 1e-3
    };
    // The part it fits in, else the last part starting at or above its top.
    let part = (0..parts).find(|&i| fits(i)).unwrap_or_else(|| (0..parts).rev().find(|&i| split_top(i, height) <= top + 1e-3).unwrap_or(0));
    let start = split_top(part, height);
    for i in [1, 3] {
        q.uv[i] = (px(q.uv[i]) - start) / SPLIT_ROWS * 16.0;
    }
    q.sprite = format!("{}__p{}", q.sprite, part).into();
}

/// Matches a render controller bone pattern: `*` matches any run of
/// characters (`*`, `arm*`, `*rightarm*`, `head`).
pub fn pattern_matches(p: &str, name: &str) -> bool {
    if p == "*" {
        return true;
    }
    // No allocation: it runs for every bone of every layer each frame.
    let Some(star) = p.find('*') else { return p == name };
    let last_star = p.rfind('*').unwrap_or(star);
    let (first, last) = (&p[..star], &p[last_star + 1..]);
    if !name.starts_with(first) || name.len() < first.len() + last.len() || !name.ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    if last_star > star {
        for mid in p[star + 1..last_star].split('*') {
            if mid.is_empty() {
                continue;
            }
            match rest.find(mid) {
                Some(i) => rest = &rest[i + mid.len()..],
                None => return false,
            }
        }
    }
    true
}

fn rgba(c: &Color, scope: &mut Scope, env: &mut dyn Env) -> [f32; 4] {
    let mut out = [0.0; 4];
    for i in 0..4 {
        // `this` in a colour channel is the colour so far: white.
        scope.this = 1.0;
        out[i] = scope.num(&c.rgba[i], env);
    }
    out
}

fn lookup_ref(v: &Value, prefix: &str) -> Option<String> {
    let s = v.as_str()?;
    let s = s.to_ascii_lowercase();
    Some(s.strip_prefix(prefix).unwrap_or(&s).to_string())
}

pub struct RenderState {
    pub hurt: bool,
    pub on_fire: bool,
}

/// Evaluates the bundle's render controllers. `geometry` resolves a
/// geometry id (`ans:entity/zombie`) to its bones.
pub fn layers(
    b: &Bundle,
    scope: &mut Scope,
    env: &mut dyn Env,
    state: &RenderState,
    geometry: &mut dyn FnMut(&str) -> Option<Rc<Geometry>>,
) -> Vec<Layer> {
    let mut out = Vec::new();
    for rc in &b.render {
        if let Some(c) = &rc.condition {
            if !scope.run(c, env).truthy() {
                continue;
            }
        }
        if let Some(l) = layer(b, rc, scope, env, state, geometry) {
            // A multi-texture material's further textures (a horse's
            // markings, its armour) are drawn as layers over the first.
            let mut extra = Vec::new();
            for t in rc.textures.iter().skip(1) {
                let tv = scope.run(t, env);
                let Some(key) = lookup_ref(&tv, "texture.") else { continue };
                if key == "transparent" {
                    continue;
                }
                if let Some(tex) = b.textures.get(&key) {
                    // Only bones with a multi-texture material sample it (a
                    // black mask over a holstered weapon is not drawn).
                    let visible: Vec<bool> = l.visible.iter().zip(&l.multi).map(|(v, m)| *v && *m).collect();
                    if *tex != l.texture && visible.iter().any(|v| *v) {
                        extra.push(Layer { texture: tex.clone(), visible, ..l.clone() });
                    }
                }
            }
            for l in std::iter::once(l).chain(extra) {
                // Two controllers drawing the same thing draw it once.
                let same = |o: &Layer| o.geometry == l.geometry && o.texture == l.texture && o.visible == l.visible && o.kinds == l.kinds && o.uv_offset == l.uv_offset;
                if !out.iter().any(same) {
                    out.push(l);
                }
            }
        }
    }
    out
}

fn layer(
    b: &Bundle,
    rc: &RenderDef,
    scope: &mut Scope,
    env: &mut dyn Env,
    state: &RenderState,
    geometry: &mut dyn FnMut(&str) -> Option<Rc<Geometry>>,
) -> Option<Layer> {
    scope.arrays.clear();
    for (name, items) in &rc.arrays {
        scope.arrays.insert(name.clone(), items.iter().map(|s| Value::Str(s.clone())).collect());
    }
    let dbg = crate::debug();
    if dbg { eprintln!("{}: arrays {:?}", rc.id, scope.arrays.keys().collect::<Vec<_>>()) }
    let gv = scope.run(&rc.geometry, env);
    let Some(gkey) = lookup_ref(&gv, "geometry.") else {
        if dbg { eprintln!("{}: geometry value {gv:?}", rc.id) }
        return None;
    };
    let Some(geo_id) = b.geometry.get(&gkey).cloned() else {
        if dbg { eprintln!("{}: no geometry {gkey}", rc.id) }
        return None;
    };
    let geo = geometry(&geo_id)?;
    let tv = rc.textures.first().map(|t| scope.run(t, env))?;
    let Some(tkey) = lookup_ref(&tv, "texture.") else {
        if dbg { eprintln!("{}: texture value {tv:?}", rc.id) }
        return None;
    };
    let Some(texture) = b.textures.get(&tkey).cloned() else {
        if dbg { eprintln!("{}: no texture {tkey}", rc.id) }
        return None;
    };
    let n = geo.bones.len();
    // A controller without `materials` draws with the bundle's default one.
    let base = if rc.materials.is_empty() { b.materials.get("default") } else { None };
    let mut kinds = vec![kind_of(base); n];
    let mut multi = vec![base.is_some_and(|m| m.multi_texture || m.bedrock.contains("multitexture")); n];
    for (pat, m) in &rc.materials {
        let mv = scope.run(m, env);
        let mat = lookup_ref(&mv, "material.").and_then(|k| b.materials.get(&k));
        let k = kind_of(mat);
        let mt = mat.is_some_and(|m| m.multi_texture || m.bedrock.contains("multitexture"));
        for &i in geo.matching(pat).iter() {
            kinds[i] = k;
            multi[i] = mt;
        }
    }
    let mut own = vec![true; n];
    for (pat, cond) in &rc.part_visibility {
        // A rule is evaluated once and applies to every bone it matches.
        let on = scope.run(cond, env).truthy();
        if dbg && on { eprintln!("{}: rule {pat} on", rc.id) }
        for &i in geo.matching(pat).iter() {
            own[i] = on;
        }
    }
    // Bedrock's part_visibility hides a bone's own cubes only: an arm stays
    // when the body around it is hidden (first person). A bone the model
    // never renders hides its children too.
    let mut never = vec![false; n];
    for &i in &geo.order {
        never[i] = geo.bones[i].hidden || geo.bones[i].parent.is_some_and(|p| never[p]);
    }
    let visible: Vec<bool> = (0..n).map(|i| own[i] && !never[i]).collect();
    // Colours: the base colour multiplies; overlay, hurt and fire colours
    // mix toward their rgb by their alpha (Bedrock's overlay).
    let mut rgb = [1.0f32; 3];
    if let Some(c) = &rc.color {
        let c = rgba(c, scope, env);
        for i in 0..3 {
            rgb[i] *= c[i];
        }
    }
    let mut mix = |c: [f32; 4]| {
        let a = c[3].clamp(0.0, 1.0);
        for i in 0..3 {
            rgb[i] = rgb[i] * (1.0 - a) + rgb[i] * c[i] * a;
        }
    };
    if let Some(c) = &rc.overlay_color {
        mix(rgba(c, scope, env));
    }
    if state.hurt {
        // Bedrock's default hurt colour, unless the controller has its own.
        let c = match &rc.is_hurt_color {
            Some(c) => rgba(c, scope, env),
            None => [1.0, 0.0, 0.0, 0.5],
        };
        mix(c);
    }
    if state.on_fire {
        if let Some(c) = &rc.on_fire_color {
            mix(rgba(c, scope, env));
        }
    }
    let to = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
    let tint = (to(rgb[0]) << 16) | (to(rgb[1]) << 8) | to(rgb[2]);
    let (uv_offset, uv_scale) = match &rc.uv_anim {
        Some((o, s)) => (
            [scope.num(&o[0], env), scope.num(&o[1], env)],
            [scope.num(&s[0], env), scope.num(&s[1], env)],
        ),
        None => ([0.0; 2], [1.0; 2]),
    };
    Some(Layer {
        controller: rc.id.clone(),
        geometry: geo_id,
        texture,
        kinds,
        multi,
        visible,
        tint,
        full_bright: rc.ignore_lighting,
        uv_offset,
        uv_scale,
    })
}

/// Bone matrices (Bedrock model space, pixels) for `geo` posed by `pose`.
/// A bone in `relative` (`relative_to_entity`) keeps its place on its
/// parent but turns relative to the entity, not to the parent.
pub fn bone_matrices(geo: &Geometry, pose: &Pose, relative: &[Rc<str>]) -> Vec<Mat4> {
    bone_matrices_bound(geo, pose, relative, &[])
}

/// `bone_matrices` with some bones following another model's bone: an
/// attachable's bound bone takes the holder's bone matrix as its parent
/// (`bone_bindings`).
pub fn bone_matrices_bound(geo: &Geometry, pose: &Pose, relative: &[Rc<str>], bound: &[(usize, Mat4)]) -> Vec<Mat4> {
    let mut m = vec![Mat4::IDENTITY; geo.bones.len()];
    for &i in &geo.order {
        let b = &geo.bones[i];
        let mut p = pose.get(&b.name);
        // A script that divides by zero (a camera query the game can't answer yet) gives NaN:
        // keep the bone at rest rather than lose the whole model.
        for (k, v) in p.iter_mut().enumerate() {
            if !v.is_finite() {
                *v = if k >= 6 { 1.0 } else { 0.0 };
            }
        }
        let mut parent = match bound.iter().find(|(j, _)| *j == i) {
            Some((_, h)) => *h,
            None => b.parent.map(|p| m[p]).unwrap_or(Mat4::IDENTITY),
        };
        if !relative.is_empty() && relative.iter().any(|r| **r == *b.name) {
            let at = parent.point(b.pivot);
            parent = Mat4::translate([at[0] - b.pivot[0], at[1] - b.pivot[1], at[2] - b.pivot[2]]);
        }
        let rot = [b.rotation[0] + p[0], b.rotation[1] + p[1], b.rotation[2] + p[2]];
        m[i] = parent
            .mul(&Mat4::translate([p[3], p[4], p[5]]))
            .mul(&Mat4::translate(b.pivot))
            .mul(&Mat4::bedrock_rotation(rot))
            .mul(&Mat4::scale([p[6], p[7], p[8]]))
            .mul(&Mat4::translate([-b.pivot[0], -b.pivot[1], -b.pivot[2]]));
    }
    m
}

/// The `rightitem` / `leftitem` bones' frames in Horizon's mob frame (see
/// `Host::hands`): Bedrock's pixels and -z front conjugated away, so the
/// rotation is proper and the unit is a block (times the model's scale).
pub fn hand_frames(geo: &Geometry, bones: &[Mat4], scale: [f32; 3]) -> [Option<Mat4>; 2] {
    let f = Mat4::scale([scale[0] / 16.0, scale[1] / 16.0, -scale[2] / 16.0]);
    let f_inv = Mat4::scale([16.0, 16.0, -16.0]);
    let find = |names: &[&str]| {
        geo.bones.iter().position(|b| names.iter().any(|n| b.name.eq_ignore_ascii_case(n))).map(|i| {
            f.mul(&bones[i]).mul(&Mat4::translate(geo.bones[i].pivot)).mul(&f_inv)
        })
    };
    [find(&["rightitem", "right_item"]), find(&["leftitem", "left_item"])]
}

/// A worn armour piece: 0 head, 1 chest, 2 legs, 3 feet, with its texture
/// (`minecraft:textures/entity/equipment/humanoid/iron.png`) and 0xRRGGBB tint.
pub struct ArmorPiece {
    pub slot: usize,
    pub texture: String,
    pub tint: u32,
}

/// Vanilla's humanoid armour (`HumanoidArmorLayer`: 64x32 textures, the
/// outer layer inflated 1, leggings 0.5) on the player model's head, body,
/// arm and leg bones as they are posed: the jar's `EquipmentLayer` draws it
/// there, since a Bedrock model has no armour of its own.
pub fn armor_quads(geo: &Geometry, bones: &[Mat4], scale: [f32; 3], pieces: &[ArmorPiece], out: &mut Vec<Quad>) {
    use crate::geo::{box_uv, Cube};
    let cube = |from: [f32; 3], size: [f32; 3], inflate: f32, uv: [f32; 2], mirror: bool| Cube {
        from,
        size,
        inflate,
        pivot: [0.0; 3],
        rotation: [0.0; 3],
        uv: box_uv(uv[0], uv[1], size, mirror),
    };
    for p in pieces {
        let mut g = geo.clone();
        g.texture_size = [64.0, 32.0];
        for b in &mut g.bones {
            b.cubes.clear();
        }
        // A morph's bones carry the pack's prefix (`egjh9head`): the part's
        // bone is the one named so, or ending so after a short prefix.
        let find = |g: &Geometry, bone: &str| {
            g.bone(bone).or_else(|| {
                g.bones.iter().position(|b| {
                    let n = b.name.to_ascii_lowercase();
                    n.len() > bone.len() && n.len() <= bone.len() + 6 && n.ends_with(bone)
                })
            })
        };
        // Each piece sits where vanilla's HumanoidModel part sits on its
        // pivot (the head's box 8 px up from the neck, an arm's 2 px below
        // the shoulder...): on the player's bones that is the vanilla place,
        // and on a morph's bones it follows them as the jar's armour does.
        // `EquipmentLayer.fit`: the piece is stretched so the vanilla part's
        // box (`base`, without the armour's inflate) lands on the bone's first
        // box, axis by axis; a bone without boxes takes it unstretched.
        let mut put = |bone: &str, vanilla_pivot: [f32; 3], base: ([f32; 3], [f32; 3]), c: Cube| {
            let Some(i) = find(&g, bone) else { return };
            let p = g.bones[i].pivot;
            let mut c = c;
            let first = geo.bones[i].cubes.first().map(|b| {
                let lo: [f32; 3] = std::array::from_fn(|k| b.from[k].min(b.from[k] + b.size[k]) - p[k]);
                let hi: [f32; 3] = std::array::from_fn(|k| b.from[k].max(b.from[k] + b.size[k]) - p[k]);
                (lo, hi)
            });
            let vlo: [f32; 3] = std::array::from_fn(|k| base.0[k] - vanilla_pivot[k]);
            let vhi: [f32; 3] = std::array::from_fn(|k| base.0[k] + base.1[k] - vanilla_pivot[k]);
            let fits = first.filter(|(lo, hi)| (0..3).all(|k| hi[k] - lo[k] > 1e-4 && vhi[k] - vlo[k] > 1e-4));
            match fits {
                Some((blo, bhi)) => {
                    // Its box (inflate included) mapped from the vanilla box onto the bone's.
                    for k in 0..3 {
                        let s = (bhi[k] - blo[k]) / (vhi[k] - vlo[k]);
                        let (vc, bc) = ((vlo[k] + vhi[k]) / 2.0, (blo[k] + bhi[k]) / 2.0);
                        let lo = c.from[k] - c.inflate - vanilla_pivot[k];
                        let hi = c.from[k] + c.size[k] + c.inflate - vanilla_pivot[k];
                        let (nlo, nhi) = (bc + (lo - vc) * s, bc + (hi - vc) * s);
                        c.from[k] = p[k] + nlo;
                        c.size[k] = nhi - nlo;
                    }
                    c.inflate = 0.0;
                }
                None => {
                    for k in 0..3 {
                        c.from[k] += p[k] - vanilla_pivot[k];
                    }
                }
            }
            g.bones[i].cubes.push(c);
        };
        const HEAD: [f32; 3] = [0.0, 24.0, 0.0];
        const BODY: [f32; 3] = [0.0, 24.0, 0.0];
        const RIGHT_ARM: [f32; 3] = [-5.0, 22.0, 0.0];
        const LEFT_ARM: [f32; 3] = [5.0, 22.0, 0.0];
        const RIGHT_LEG: [f32; 3] = [-1.9, 12.0, 0.0];
        const LEFT_LEG: [f32; 3] = [1.9, 12.0, 0.0];
        match p.slot {
            0 => put("head", HEAD, ([-4.0, 24.0, -4.0], [8.0, 8.0, 8.0]), cube([-4.0, 24.0, -4.0], [8.0, 8.0, 8.0], 1.0, [0.0, 0.0], false)),
            1 => {
                put("body", BODY, ([-4.0, 12.0, -2.0], [8.0, 12.0, 4.0]), cube([-4.0, 12.0, -2.0], [8.0, 12.0, 4.0], 1.0, [16.0, 16.0], false));
                put("rightarm", RIGHT_ARM, ([-8.0, 12.0, -2.0], [4.0, 12.0, 4.0]), cube([-8.0, 12.0, -2.0], [4.0, 12.0, 4.0], 1.0, [40.0, 16.0], false));
                put("leftarm", LEFT_ARM, ([4.0, 12.0, -2.0], [4.0, 12.0, 4.0]), cube([4.0, 12.0, -2.0], [4.0, 12.0, 4.0], 1.0, [40.0, 16.0], true));
            }
            2 => {
                put("body", BODY, ([-4.0, 12.0, -2.0], [8.0, 12.0, 4.0]), cube([-4.0, 12.0, -2.0], [8.0, 12.0, 4.0], 0.5, [16.0, 16.0], false));
                put("rightleg", RIGHT_LEG, ([-3.9, 0.0, -2.0], [4.0, 12.0, 4.0]), cube([-3.9, 0.0, -2.0], [4.0, 12.0, 4.0], 0.5, [0.0, 16.0], false));
                put("leftleg", LEFT_LEG, ([-0.1, 0.0, -2.0], [4.0, 12.0, 4.0]), cube([-0.1, 0.0, -2.0], [4.0, 12.0, 4.0], 0.5, [0.0, 16.0], true));
            }
            _ => {
                put("rightleg", RIGHT_LEG, ([-3.9, 0.0, -2.0], [4.0, 12.0, 4.0]), cube([-3.9, 0.0, -2.0], [4.0, 12.0, 4.0], 1.0, [0.0, 16.0], false));
                put("leftleg", LEFT_LEG, ([-0.1, 0.0, -2.0], [4.0, 12.0, 4.0]), cube([-0.1, 0.0, -2.0], [4.0, 12.0, 4.0], 1.0, [0.0, 16.0], true));
            }
        }
        let n = g.bones.len();
        let layer = Layer {
            controller: String::new(),
            geometry: g.id.clone(),
            texture: p.texture.clone(),
            kinds: vec![Kind::Cutout; n],
            multi: vec![false; n],
            visible: vec![true; n],
            tint: p.tint,
            full_bright: false,
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
        };
        mesh(&g, &layer, bones, scale, 0.0, out);
    }
}

fn corners(f: Face, a: [f32; 3], b: [f32; 3]) -> [[f32; 3]; 4] {
    let [x0, y0, z0] = a;
    let [x1, y1, z1] = b;
    match f {
        Face::North => [[x0, y1, z0], [x0, y0, z0], [x1, y0, z0], [x1, y1, z0]],
        Face::South => [[x1, y1, z1], [x1, y0, z1], [x0, y0, z1], [x0, y1, z1]],
        Face::West => [[x0, y1, z1], [x0, y0, z1], [x0, y0, z0], [x0, y1, z0]],
        Face::East => [[x1, y1, z0], [x1, y0, z0], [x1, y0, z1], [x1, y1, z1]],
        Face::Up => [[x0, y1, z1], [x0, y1, z0], [x1, y1, z0], [x1, y1, z1]],
        Face::Down => [[x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]],
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Quads for `layer` posed by `bones`, in Horizon's mob frame (blocks, +z
/// the way the mob faces, +x its left), scaled by `scale`.
/// `bias` (blocks) pushes every face out along its normal: a later layer on
/// the same geometry (cracks, glow, a saddle) sits just above the one under
/// it instead of fighting it for depth.
pub fn mesh(geo: &Geometry, layer: &Layer, bones: &[Mat4], scale: [f32; 3], bias: f32, out: &mut Vec<Quad>) {
    mesh_cubes(geo, layer, bones, scale, bias, out, &mut Vec::new());
}

/// `mesh`, also noting the quads (start, end) each cube drew in `spans`.
pub fn mesh_cubes(geo: &Geometry, layer: &Layer, bones: &[Mat4], scale: [f32; 3], bias: f32, out: &mut Vec<Quad>, spans: &mut Vec<(usize, usize)>) {
    let sprite: Rc<str> = Rc::from(sprite_name(&layer.texture));
    let [tw, th] = geo.texture_size;
    // Bedrock model space to Horizon: pixels to blocks, front -z to +z.
    let to_h = Mat4::scale([scale[0] / 16.0, scale[1] / 16.0, -scale[2] / 16.0]);
    for (bi, bone) in geo.bones.iter().enumerate() {
        if !layer.visible[bi] || matches!(layer.kinds[bi], Kind::Hidden | Kind::Blended) {
            continue;
        }
        let full_bright = layer.full_bright || matches!(layer.kinds[bi], Kind::Emissive);
        // A glow texture: its lit half, then its glowing half full bright
        // (the pack ships both when some texel glows; see `Host`).
        let variants: Vec<(Rc<str>, bool)> = if layer.kinds[bi] == Kind::EmissiveAlpha {
            vec![(format!("{sprite}{LIT}").into(), full_bright), (format!("{sprite}{GLOW}").into(), true)]
        } else {
            vec![(sprite.clone(), full_bright)]
        };
        for c in &bone.cubes {
            let cube_start = out.len();
            let cube_m = Mat4::translate(c.pivot)
                .mul(&Mat4::bedrock_rotation(c.rotation))
                .mul(&Mat4::translate([-c.pivot[0], -c.pivot[1], -c.pivot[2]]));
            let m = to_h.mul(&bones[bi]).mul(&cube_m);
            let mut lo = [c.from[0] - c.inflate, c.from[1] - c.inflate, c.from[2] - c.inflate];
            let mut hi = [
                c.from[0] + c.size[0] + c.inflate,
                c.from[1] + c.size[1] + c.inflate,
                c.from[2] + c.size[2] + c.inflate,
            ];
            // A later layer's lift: a box grows by it all round (its faces
            // still meet at the edges; pushing each face out along its normal
            // opened a crack along every edge, the first person arm's holes);
            // only a flat one (a plane) is pushed along its normal below.
            let solid = c.size.iter().all(|s| s.abs() > 1e-6);
            if bias != 0.0 && solid {
                let k = (scale[0].abs() + scale[1].abs() + scale[2].abs()) / 3.0;
                let grow = bias * 16.0 / k.max(1e-6);
                for a in 0..3 {
                    let out_dir = if hi[a] >= lo[a] { 1.0 } else { -1.0 };
                    lo[a] -= grow * out_dir;
                    hi[a] += grow * out_dir;
                }
            }
            let inverted = c.size.iter().filter(|s| **s < 0.0).count() % 2 == 1;
            let center = m.point([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0]);
            for (fi, f) in FACES.iter().enumerate() {
                let Some([u0, v0, u1, v1]) = c.uv[fi] else { continue };
                // A face with no area in the texture draws nothing (Bedrock).
                if u0 == u1 && v0 == v1 {
                    continue;
                }
                let mut p = corners(*f, lo, hi).map(|q| m.point(q));
                let s = |u: f32| (u / tw * layer.uv_scale[0] + layer.uv_offset[0]) * 16.0;
                let t = |v: f32| (v / th * layer.uv_scale[1] + layer.uv_offset[1]) * 16.0;
                let mut uv = [s(u0), t(v0), s(u1), t(v1)];
                // A hair inside the face's texels: at a face's edge the
                // interpolated coordinate can land on the next texel, which
                // for a skin is often an unused (black) one, drawn as a black
                // seam along the edge (the first person arm's).
                for (a, b, size) in [(0, 2, tw), (1, 3, th)] {
                    let e = 0.05 * 16.0 / size * layer.uv_scale[a % 2].abs();
                    let d = uv[b] - uv[a];
                    if d.abs() > 4.0 * e {
                        let e = e * d.signum();
                        uv[a] += e;
                        uv[b] -= e;
                    }
                }
                // Make the face front-facing for Horizon: counter-clockwise
                // seen from outside. A flat face (a plane) keeps its order.
                let n = cross(sub(p[1], p[0]), sub(p[2], p[1]));
                let fc = [0, 1, 2].map(|k| (p[0][k] + p[2][k]) / 2.0);
                let side = dot(n, sub(fc, center));
                // A cube with an odd number of negative sizes is inside out
                // (Bedrock places each face at the origin's side, facing as
                // if the size were positive): its faces face inward, so only
                // the far side shows (A&B's outlines and glass shells; seen
                // from inside, a camera would see it all around).
                if (side < 0.0) != inverted {
                    p = [p[3], p[2], p[1], p[0]];
                    uv = [uv[2], uv[1], uv[0], uv[3]];
                }
                // A flat cube (a plane: an item's sprite, a flame) has no
                // outside to face: draw it from both sides, or a mirroring
                // animation turns it away from the camera and it vanishes.
                if side.abs() <= 1e-6 * dot(n, n).sqrt().max(1e-6) {
                    for (sprite, full_bright) in &variants {
                        out.push(Quad { sprite: sprite.clone(), corners: [p[3], p[2], p[1], p[0]], uv: [uv[2], uv[1], uv[0], uv[3]], tint: layer.tint, full_bright: *full_bright });
                    }
                }
                if bias != 0.0 && !solid {
                    let n = cross(sub(p[1], p[0]), sub(p[2], p[1]));
                    let len = dot(n, n).sqrt();
                    if len > 0.0 {
                        let d = [n[0] / len * bias, n[1] / len * bias, n[2] / len * bias];
                        for c in &mut p {
                            for k in 0..3 {
                                c[k] += d[k];
                            }
                        }
                    }
                }
                for (sprite, full_bright) in &variants {
                    out.push(Quad { sprite: sprite.clone(), corners: p, uv, tint: layer.tint, full_bright: *full_bright });
                }
            }
            if out.len() > cube_start {
                spans.push((cube_start, out.len()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_lifted_box_stays_closed_at_its_edges() {
        use crate::geo::{Bone, Cube, Geometry};
        let cube = Cube { from: [-1.0, 0.0, -2.0], size: [4.0, 12.0, 4.0], inflate: 0.0, pivot: [0.0; 3], rotation: [10.0, 20.0, 5.0], uv: [Some([0.0, 0.0, 4.0, 12.0]); 6] };
        let geo = Geometry { id: "g".into(), texture_size: [64.0, 64.0], bones: vec![Bone { name: "arm".into(), cubes: vec![cube], ..Default::default() }], ..Default::default() };
        let layer = Layer {
            controller: String::new(),
            geometry: "g".into(),
            texture: "t".into(),
            kinds: vec![Kind::Cutout],
            multi: vec![false],
            visible: vec![true],
            tint: 0xFFFFFF,
            full_bright: false,
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
        };
        let mut out = Vec::new();
        mesh(&geo, &layer, &[Mat4::translate([0.0; 3])], [1.0; 3], 0.02, &mut out);
        assert_eq!(out.len(), 6);
        // Every corner is shared by three faces, exactly.
        let corners: Vec<[f32; 3]> = out.iter().flat_map(|q| q.corners).collect();
        for c in &corners {
            let same = corners.iter().filter(|d| (0..3).all(|k| (c[k] - d[k]).abs() < 1e-6)).count();
            assert_eq!(same, 3, "corner {c:?} shared by {same}");
        }
        // And it grew: wider than the 4 pixel box (0.25 blocks) by the lift.
        let xs: Vec<f32> = corners.iter().map(|c| c[1]).collect();
        let h = xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
        assert!(h > 0.75, "{h}");
    }

    #[test]
    fn a_composite_quad_draws_each_part_it_lies_in() {
        use std::collections::HashMap;
        let c = Composite {
            size: [64.0, 64.0],
            parts: vec![
                CompositePart { sprite: "entity/sheep/sheep".into(), src: None, dst: [0.0, 0.0], tinted: false },
                CompositePart { sprite: "entity/sheep/sheep_wool_undercoat".into(), src: None, dst: [0.0, 0.0], tinted: true },
                CompositePart { sprite: "entity/sheep/sheep_wool".into(), src: None, dst: [0.0, 32.0], tinted: true },
            ],
        };
        let sizes: crate::FastMap<String, [f32; 2]> = [
            ("entity/sheep/sheep".to_string(), [64.0, 32.0]),
            ("entity/sheep/sheep_wool_undercoat".to_string(), [64.0, 32.0]),
            ("entity/sheep/sheep_wool".to_string(), [64.0, 32.0]),
        ]
        .into_iter()
        .collect();
        let quad = |v0: f32, v1: f32| Quad {
            sprite: "entity/sheep/sheep".into(),
            corners: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            uv: [0.0, v0, 2.0, v1],
            tint: 0xFFFFFF,
            full_bright: false,
        };
        // The body, in the top half: the body, then the undercoat in wool colour.
        let mut out = Vec::new();
        composite_parts(&quad(0.0, 2.0), &c, &sizes, 0xFF0000, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(&*out[0].sprite, "entity/sheep/sheep");
        assert_eq!(out[0].uv, [0.0, 0.0, 2.0, 4.0]);
        assert_eq!(out[0].tint, 0xFFFFFF);
        assert_eq!(&*out[1].sprite, "entity/sheep/sheep_wool_undercoat");
        assert_eq!(out[1].tint, 0xFF0000);
        assert!(out[1].corners[0][2] != 0.0);
        // The wool, in the bottom half: only the wool, from its top.
        out.clear();
        composite_parts(&quad(8.0, 10.0), &c, &sizes, 0x00FF00, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(&*out[0].sprite, "entity/sheep/sheep_wool");
        assert_eq!(out[0].uv, [0.0, 0.0, 2.0, 4.0]);
        assert_eq!(out[0].tint, 0x00FF00);
        // Sizes not known yet: the quad as it was.
        out.clear();
        composite_parts(&quad(8.0, 10.0), &c, &crate::FastMap::default(), 0, &mut out);
        assert_eq!(out, vec![quad(8.0, 10.0)]);
    }

    use super::*;
    #[test]
    fn composed_atlas_cells_point_at_vanilla_sprites() {
        let mut cells = crate::FastMap::default();
        cells.insert((16, 112), Rc::from("block/redstone_block"));
        // Texels 16..32 x 112..128 of a 1024 px texture: the whole cell.
        let (s, uv) = atlas_cell([0.25, 1.75, 0.5, 2.0], [1024.0, 1024.0], 16.0, &cells).unwrap();
        assert_eq!(&*s, "block/redstone_block");
        assert!(uv.iter().zip([0.0, 0.0, 16.0, 16.0]).all(|(a, b)| (a - b).abs() < 1e-3), "{uv:?}");
        assert!(atlas_cell([0.0, 0.0, 0.25, 0.25], [1024.0, 1024.0], 16.0, &cells).is_none());
    }

    #[test]
    fn cut_textures_point_at_their_part() {
        let q = |v0: f32, v1: f32, h: f32| {
            let mut q = Quad { sprite: "ans:entity/a".into(), corners: [[0.0; 3]; 4], uv: [0.0, 16.0 * v0 / h, 1.0, 16.0 * v1 / h], tint: 0, full_bright: false };
            to_split_part(&mut q, h);
            (q.sprite.to_string(), q.uv[1] / 16.0 * 512.0, q.uv[3] / 16.0 * 512.0)
        };
        let near = |a: (String, f32, f32), b: (&str, f32, f32)| a.0 == b.0 && (a.1 - b.1).abs() < 1e-2 && (a.2 - b.2).abs() < 1e-2;
        // 1024 rows: parts from rows 0, 256 and 512.
        assert!(near(q(10.0, 20.0, 1024.0), ("ans:entity/a__p0", 10.0, 20.0)));
        assert!(near(q(576.0, 592.0, 1024.0), ("ans:entity/a__p1", 320.0, 336.0)));
        // Across the cut at 512: wholly in the middle part.
        assert!(near(q(505.0, 517.0, 1024.0), ("ans:entity/a__p1", 249.0, 261.0)));
        assert!(near(q(900.0, 1024.0, 1024.0), ("ans:entity/a__p2", 388.0, 512.0)));
        // 880 rows: parts from 0, 256 and 368.
        assert!(near(q(700.0, 880.0, 880.0), ("ans:entity/a__p2", 332.0, 512.0)));
    }

    #[test]
    fn sprite_names() {
        assert_eq!(sprite_name("ans:textures/oreville/ans/bge.png"), "ans:entity/oreville/ans/bge");
        assert_eq!(sprite_name("minecraft:textures/entity/zombie/zombie_baby.png"), "entity/zombie/zombie_baby");
    }
    #[test]
    fn patterns() {
        assert!(pattern_matches("*", "head"));
        assert!(pattern_matches("arm*", "armleft"));
        assert!(!pattern_matches("head", "head2"));
        assert!(pattern_matches("*rightarm*", "egjh9rightarmsleeve"));
        assert!(pattern_matches("*rightarm*", "rightarm"));
        assert!(!pattern_matches("*rightarm*", "leftarm"));
        assert!(pattern_matches("e2jc_*", "e2jc_blade"));
        assert!(!pattern_matches("e2jc_*", "body"));
        assert!(pattern_matches("a*b*c", "axxbyyc"));
        assert!(!pattern_matches("a*b*c", "axxcyyb"));
        assert!(pattern_matches("a**c", "ac"));
    }
    #[test]
    fn rotation_matches_geckolib() {
        // GeckoLib turns +x of its mirrored space by its X rotation; in
        // Bedrock space a +90 x rotation tips +y towards -z... check it is a
        // proper rotation and that a pure y turn agrees with Bedrock's
        // (positive y turns the model's front, -z, towards -x).
        let m = Mat4::bedrock_rotation([0.0, 90.0, 0.0]);
        let p = m.point([0.0, 0.0, -1.0]);
        assert!((p[0] + 1.0).abs() < 1e-5, "{p:?}");
    }
}
