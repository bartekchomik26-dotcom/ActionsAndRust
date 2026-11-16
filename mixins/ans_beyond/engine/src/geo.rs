// Bedrock geometry (`minecraft:geometry`, format 1.12+ and the older 1.8
// `geometry.x` keys) as GeckoLib bakes it for Actions & Beyond.
//
// Everything stays in Bedrock's own model space: pixels, y up, the model's
// front towards -z, x the same as a vanilla `ModelPart`'s (so a right arm is
// at -x). `mesh` maps to Horizon's mob frame at the end.

use crate::json::Json;


#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Face {
    North,
    South,
    West, // -x
    East, // +x
    Up,
    Down,
}

pub const FACES: [Face; 6] = [Face::North, Face::South, Face::West, Face::East, Face::Up, Face::Down];

#[derive(Clone, Debug, Default)]
pub struct Cube {
    pub from: [f32; 3],
    pub size: [f32; 3],
    pub inflate: f32,
    pub pivot: [f32; 3],
    pub rotation: [f32; 3],
    /// Per face: [u0, v0, u1, v1] in texture pixels, None for a face left out.
    pub uv: [Option<[f32; 4]>; 6],
}

#[derive(Clone, Debug, Default)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    pub pivot: [f32; 3],
    pub rotation: [f32; 3],
    pub cubes: Vec<Cube>,
    pub locators: Vec<(String, [f32; 3])>,
    /// `neverRender` bones (GeckoLib) and bones a pack hides by default.
    pub hidden: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Geometry {
    pub id: String,
    pub texture_size: [f32; 2],
    pub bones: Vec<Bone>,
    pub by_name: crate::FastMap<String, usize>,
    /// Bones in an order where every parent comes before its children.
    pub order: Vec<usize>,
    /// The bones each render controller pattern (`*arm*`) names, worked out
    /// once: matching every bone against every pattern each frame was a sixth
    /// of a mob's draw.
    pub matches: std::cell::RefCell<crate::FastMap<String, std::rc::Rc<[usize]>>>,
}

impl Geometry {
    /// The bones `pattern` matches (`render::pattern_matches`), by index.
    pub fn matching(&self, pattern: &str) -> std::rc::Rc<[usize]> {
        if let Some(m) = self.matches.borrow().get(pattern) {
            return m.clone();
        }
        let m: std::rc::Rc<[usize]> = self
            .bones
            .iter()
            .enumerate()
            .filter(|(_, b)| crate::render::pattern_matches(pattern, &b.name))
            .map(|(i, _)| i)
            .collect();
        self.matches.borrow_mut().insert(pattern.to_string(), m.clone());
        m
    }
    pub fn bone(&self, name: &str) -> Option<usize> {
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }
}

fn face_key(f: Face) -> &'static str {
    match f {
        Face::North => "north",
        Face::South => "south",
        Face::West => "west",
        Face::East => "east",
        Face::Up => "up",
        Face::Down => "down",
    }
}

/// Box UV, as the net Bedrock and vanilla unfold a cube onto a texture.
pub fn box_uv(u: f32, v: f32, size: [f32; 3], mirror: bool) -> [Option<[f32; 4]>; 6] {
    let [w, h, d] = size;
    let mut west = [u, v + d, u + d, v + d + h];
    let mut east = [u + d + w, v + d, u + d + w + d, v + d + h];
    let mut north = [u + d, v + d, u + d + w, v + d + h];
    let mut south = [u + d + w + d, v + d, u + d + w + d + w, v + d + h];
    let mut up = [u + d, v, u + d + w, v + d];
    // The bottom is read upside down (GeckoLib's `{sizeX, -sizeZ}`).
    let mut down = [u + d + w, v + d, u + d + w + w, v];
    if mirror {
        std::mem::swap(&mut west, &mut east);
        for r in [&mut west, &mut east, &mut north, &mut south, &mut up, &mut down] {
            r.swap(0, 2);
        }
    }
    [Some(north), Some(south), Some(west), Some(east), Some(up), Some(down)]
}

fn f3(j: Option<&Json>, d: [f32; 3]) -> [f32; 3] {
    j.and_then(Json::f3).unwrap_or(d)
}

pub fn parse_file(j: &Json) -> Vec<Geometry> {
    let mut out = Vec::new();
    if let Some(list) = j.get("minecraft:geometry") {
        for g in list.as_arr() {
            let desc = g.get("description");
            let id = desc.and_then(|d| d.get("identifier")).and_then(Json::as_str).unwrap_or("").to_string();
            let tw = desc.and_then(|d| d.get("texture_width")).and_then(Json::as_f64).unwrap_or(64.0) as f32;
            let th = desc.and_then(|d| d.get("texture_height")).and_then(Json::as_f64).unwrap_or(64.0) as f32;
            out.push(parse_geo(id, [tw, th], g.get("bones")));
        }
    }
    // Format 1.8: { "geometry.name": { texturewidth, bones } }.
    for (k, g) in j.entries() {
        if k.starts_with("geometry.") {
            let name = k.split(':').next().unwrap_or(k).to_string();
            let tw = g.get("texturewidth").and_then(Json::as_f64).unwrap_or(64.0) as f32;
            let th = g.get("textureheight").and_then(Json::as_f64).unwrap_or(64.0) as f32;
            out.push(parse_geo(name, [tw, th], g.get("bones")));
        }
    }
    out
}

fn parse_geo(id: String, texture_size: [f32; 2], bones: Option<&Json>) -> Geometry {
    let mut g = Geometry { id, texture_size, ..Default::default() };
    let mut parents = Vec::new();
    for b in bones.map(Json::as_arr).unwrap_or(&[]) {
        let name = b.get("name").and_then(Json::as_str).unwrap_or("").to_ascii_lowercase();
        let mirror_bone = b.get("mirror").and_then(Json::as_bool).unwrap_or(false);
        let bone_inflate = b.get("inflate").and_then(Json::as_f64).unwrap_or(0.0) as f32;
        let mut bone = Bone {
            name: name.clone(),
            pivot: f3(b.get("pivot"), [0.0; 3]),
            rotation: f3(b.get("rotation"), [0.0; 3]),
            hidden: b.get("neverRender").and_then(Json::as_bool).unwrap_or(false),
            ..Default::default()
        };
        // 1.8's `bind_pose_rotation` is the rest rotation too.
        if let Some(r) = b.get("bind_pose_rotation").and_then(Json::f3) {
            bone.rotation = r;
        }
        for c in b.get("cubes").map(Json::as_arr).unwrap_or(&[]) {
            let size = f3(c.get("size"), [0.0; 3]);
            let from = f3(c.get("origin"), [0.0; 3]);
            let mirror = c.get("mirror").and_then(Json::as_bool).unwrap_or(mirror_bone);
            let uv = match c.get("uv") {
                Some(Json::Arr(a)) if a.len() >= 2 => {
                    box_uv(a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32, size, mirror)
                }
                Some(per @ Json::Obj(_)) => {
                    let mut faces = [None; 6];
                    for (i, f) in FACES.iter().enumerate() {
                        if let Some(fj) = per.get(face_key(*f)) {
                            let a = fj.get("uv").map(Json::as_arr).unwrap_or(&[]);
                            let s = fj.get("uv_size").map(Json::as_arr).unwrap_or(&[]);
                            if a.len() >= 2 && s.len() >= 2 {
                                let (u, v) = (a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32);
                                let (w, h) = (s[0].as_f64().unwrap_or(0.0) as f32, s[1].as_f64().unwrap_or(0.0) as f32);
                                faces[i] = Some([u, v, u + w, v + h]);
                            }
                        }
                    }
                    faces
                }
                _ => box_uv(0.0, 0.0, size, mirror),
            };
            bone.cubes.push(Cube {
                from,
                size,
                inflate: c.get("inflate").and_then(Json::as_f64).map(|x| x as f32).unwrap_or(bone_inflate),
                pivot: f3(c.get("pivot"), [from[0] + size[0] / 2.0, from[1] + size[1] / 2.0, from[2] + size[2] / 2.0]),
                rotation: f3(c.get("rotation"), [0.0; 3]),
                uv,
            });
        }
        for (k, v) in b.get("locators").map(Json::entries).unwrap_or(&[]) {
            let p = v.f3().or_else(|| v.get("offset").and_then(Json::f3)).unwrap_or([0.0; 3]);
            bone.locators.push((k.to_ascii_lowercase(), p));
        }
        parents.push(b.get("parent").and_then(Json::as_str).map(str::to_ascii_lowercase));
        g.by_name.insert(name, g.bones.len());
        g.bones.push(bone);
    }
    for (i, p) in parents.into_iter().enumerate() {
        // A bone naming itself or a missing parent is a root.
        g.bones[i].parent = p.and_then(|p| g.by_name.get(&p).copied()).filter(|&p| p != i);
    }
    // Topological order; a cycle's bones are made roots.
    let n = g.bones.len();
    let mut state = vec![0u8; n];
    fn visit(i: usize, g: &mut Geometry, state: &mut [u8], out: &mut Vec<usize>) {
        if state[i] == 2 {
            return;
        }
        if state[i] == 1 {
            g.bones[i].parent = None;
            return;
        }
        state[i] = 1;
        if let Some(p) = g.bones[i].parent {
            visit(p, g, state, out);
            if state[i] == 2 {
                return;
            }
        }
        state[i] = 2;
        out.push(i);
    }
    let mut order = Vec::with_capacity(n);
    for i in 0..n {
        visit(i, &mut g, &mut state, &mut order);
    }
    g.order = order;
    g
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zombie_like_geometry() {
        let j = crate::json::parse(r#"{"format_version":"1.12.0","minecraft:geometry":[{"description":{"identifier":"geometry.t","texture_width":64,"texture_height":32},
        "bones":[{"name":"leftarm","parent":"body","pivot":[5,22,0],"mirror":true,"cubes":[{"origin":[4,12,-2],"size":[4,12,4],"uv":[40,16]}]},
                 {"name":"body","pivot":[0,24,0]}]}]}"#).unwrap();
        let g = &parse_file(&j)[0];
        assert_eq!(g.texture_size, [64.0, 32.0]);
        let arm = &g.bones[g.bone("leftarm").unwrap()];
        assert_eq!(arm.parent, g.bone("body"));
        // parents come first
        assert_eq!(g.order, vec![1, 0]);
        // mirrored: the -x face takes the +x strip, flipped
        assert_eq!(arm.cubes[0].uv[2], Some([52.0, 20.0, 48.0, 32.0]));
        assert_eq!(arm.cubes[0].uv[0], Some([48.0, 20.0, 44.0, 32.0]));
    }
}
