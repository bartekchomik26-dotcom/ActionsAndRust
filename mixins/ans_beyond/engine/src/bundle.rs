// An `ans:bedrock_entity` file (Actions & Beyond's pre-resolved Bedrock
// client entity): geometry, textures and materials by short name, the
// entity's scripts, its animations and animation controllers, and its
// render controllers. Mirrors `com.ans.client.bedrock.EntityBundle`.

use crate::anim::{self, Animation, Event, Loop};
use crate::json::Json;
use crate::molang::{self, Script};
use std::cell::RefCell;

use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct Entry {
    pub target: Rc<str>,
    pub weight: Option<Rc<Script>>,
}

#[derive(Debug)]
pub struct AnimDef {
    pub key: String,
    pub animation: String,
    pub looping: Option<Loop>,
    pub length: Option<f64>,
    pub anim_time_update: Option<Script>,
    pub blend_weight: Option<Script>,
    pub start_delay: Option<Script>,
    pub loop_delay: Option<Script>,
    pub override_previous: bool,
    pub relative_to_entity: Vec<Rc<str>>,
    pub timeline: Vec<Event>,
    pub sounds: Vec<Event>,
    pub particles: Vec<Event>,
}

#[derive(Clone, Debug, Default)]
pub struct Blend {
    pub seconds: f64,
    pub curve: Vec<(f64, f64)>,
}

impl Blend {
    pub const NONE: Blend = Blend { seconds: 0.0, curve: Vec::new() };
    /// The weight left on the state being left, `t` seconds in.
    pub fn outgoing(&self, t: f64) -> f64 {
        if !self.curve.is_empty() {
            let c = &self.curve;
            if t <= c[0].0 {
                return c[0].1;
            }
            for i in 1..c.len() {
                if t <= c[i].0 {
                    let (t0, v0) = c[i - 1];
                    let (t1, v1) = c[i];
                    let f = if t1 == t0 { 1.0 } else { (t - t0) / (t1 - t0) };
                    return v0 + f * (v1 - v0);
                }
            }
            return c[c.len() - 1].1;
        }
        if self.seconds <= 0.0 { 0.0 } else { (1.0 - t / self.seconds).max(0.0) }
    }
}

#[derive(Debug)]
pub struct ParticleRef {
    pub effect: String,
    pub locator: Option<String>,
    pub script: Option<Script>,
}

#[derive(Debug)]
pub struct State {
    pub name: String,
    pub animations: Vec<Entry>,
    pub transitions: Vec<(Rc<str>, Script)>,
    pub blend: Blend,
    pub on_entry: Vec<Script>,
    pub on_exit: Vec<Script>,
    pub particles: Vec<ParticleRef>,
    pub sounds: Vec<String>,
}

#[derive(Debug)]
pub struct ControllerDef {
    pub key: String,
    pub initial: Rc<str>,
    pub states: crate::FastMap<Rc<str>, Rc<State>>,
}

#[derive(Debug)]
pub enum Target {
    Anim(Rc<AnimDef>),
    Ctrl(Rc<ControllerDef>),
}

#[derive(Debug, Clone)]
pub struct Color {
    pub rgba: [Script; 4],
}

#[derive(Debug)]
pub struct RenderDef {
    pub id: String,
    pub condition: Option<Script>,
    pub arrays: Vec<(Rc<str>, Vec<Rc<str>>)>,
    pub geometry: Script,
    pub textures: Vec<Script>,
    /// (bone pattern, material expression)
    pub materials: Vec<(String, Script)>,
    pub part_visibility: Vec<(String, Script)>,
    pub color: Option<Color>,
    pub overlay_color: Option<Color>,
    pub on_fire_color: Option<Color>,
    pub is_hurt_color: Option<Color>,
    pub ignore_lighting: bool,
    pub light_multiplier: Option<Script>,
    pub uv_anim: Option<([Script; 2], [Script; 2])>,
}

#[derive(Debug, Clone)]
pub struct Material {
    pub bedrock: String,
    pub render_type: String,
    pub uv_anim: bool,
    pub bilinear: bool,
    /// Samples its controller's further textures (`multi_texture`: a horse's
    /// markings and armour). Other materials' second texture is an
    /// enchantment glint mask (`glint`), which is not a layer to draw.
    pub multi_texture: bool,
    pub glint: bool,
}

/// Reads a resource by namespaced id and kind (`geo`, `animation`).
pub trait Assets {
    fn json(&self, path: &str) -> Option<Json>;
}

pub struct Bundle {
    pub id: String,
    pub animation_file: String,
    pub geometry: crate::FastMap<String, String>,
    pub textures: crate::FastMap<String, String>,
    pub materials: crate::FastMap<String, Material>,
    pub targets: crate::FastMap<String, Target>,
    pub initialize: Vec<Script>,
    pub pre_animation: Vec<Script>,
    /// An attachable's `parent_setup`: run on its holder's variables before
    /// the holder is drawn.
    pub parent_setup: Vec<Script>,
    pub animate: Vec<Entry>,
    pub scale: Option<Script>,
    pub scale_x: Option<Script>,
    pub scale_y: Option<Script>,
    pub scale_z: Option<Script>,
    pub render: Vec<RenderDef>,
    pub sound_effects: crate::FastMap<String, String>,
    pub particle_effects: crate::FastMap<String, String>,
    /// An attachable's `bone_bindings`: per geometry id, (its bone, the
    /// holder's bone it follows, as an expression giving the bone's name).
    pub bindings: crate::FastMap<String, Vec<(String, Script)>>,
    anims: RefCell<crate::FastMap<String, Rc<Animation>>>,
}

impl std::fmt::Debug for Bundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bundle({})", self.id)
    }
}

fn scripts(j: Option<&Json>) -> Vec<Script> {
    match j {
        Some(Json::Arr(a)) => a.iter().filter_map(Json::as_str).map(molang::compile_or_zero).collect(),
        Some(Json::Str(s)) => vec![molang::compile_or_zero(s)],
        _ => Vec::new(),
    }
}

fn entry_list(j: Option<&Json>) -> Vec<Entry> {
    let mut out = Vec::new();
    for e in j.map(Json::as_arr).unwrap_or(&[]) {
        match e {
            Json::Str(s) => out.push(Entry { target: Rc::from(s.as_str()), weight: None }),
            Json::Obj(kv) => {
                for (k, v) in kv {
                    out.push(Entry { target: Rc::from(k.as_str()), weight: Some(Rc::new(anim::script(v))) });
                }
            }
            _ => {}
        }
    }
    out
}

fn ids(j: Option<&Json>) -> crate::FastMap<String, String> {
    j.map(Json::entries)
        .unwrap_or(&[])
        .iter()
        .filter_map(|(k, v)| Some((k.to_ascii_lowercase(), v.as_str()?.to_string())))
        .collect()
}

fn blend(j: Option<&Json>) -> Blend {
    match j {
        Some(Json::Num(n)) => Blend { seconds: *n, curve: Vec::new() },
        Some(Json::Obj(kv)) => {
            let mut curve: Vec<(f64, f64)> =
                kv.iter().filter_map(|(k, v)| Some((k.trim().parse().ok()?, v.as_f64()?))).collect();
            curve.sort_by(|a, b| a.0.total_cmp(&b.0));
            Blend { seconds: curve.last().map(|c| c.0).unwrap_or(0.0), curve }
        }
        _ => Blend::NONE,
    }
}

fn color(j: Option<&Json>) -> Option<Color> {
    let j = j?;
    let c = |k: &str| j.get(k).map(anim::script).unwrap_or_else(|| Script::constant(if k == "a" { 1.0 } else { 0.0 }));
    Some(Color { rgba: [c("r"), c("g"), c("b"), c("a")] })
}

fn rules(j: Option<&Json>) -> Vec<(String, Script)> {
    let mut out = Vec::new();
    for r in j.map(Json::as_arr).unwrap_or(&[]) {
        match r {
            Json::Arr(p) if p.len() >= 2 => {
                if let Some(pat) = p[0].as_str() {
                    out.push((pat.to_ascii_lowercase(), anim::script(&p[1])));
                }
            }
            Json::Obj(kv) => {
                for (k, v) in kv {
                    out.push((k.to_ascii_lowercase(), anim::script(v)));
                }
            }
            _ => {}
        }
    }
    out
}

fn render_def(j: &Json) -> RenderDef {
    let mut arrays = Vec::new();
    for (_, group) in j.get("arrays").map(Json::entries).unwrap_or(&[]) {
        for (name, list) in group.entries() {
            let items = list.as_arr().iter().filter_map(Json::as_str).map(|s| Rc::from(s.to_ascii_lowercase())).collect();
            arrays.push((Rc::from(name.to_ascii_lowercase()), items));
        }
    }
    if let Some(Json::Obj(kv)) = j.get("arrays") {
        for (name, list) in kv {
            if let Json::Arr(a) = list {
                let items = a.iter().filter_map(Json::as_str).map(|s| Rc::from(s.to_ascii_lowercase())).collect();
                arrays.push((Rc::from(name.to_ascii_lowercase()), items));
            }
        }
    }
    let uv_anim = j.get("uv_anim").map(|u| {
        let pair = |k: &str, d: f32| {
            let a = u.get(k).map(Json::as_arr).unwrap_or(&[]);
            let s = |i: usize| a.get(i).map(anim::script).unwrap_or_else(|| Script::constant(d));
            [s(0), s(1)]
        };
        (pair("offset", 0.0), pair("scale", 1.0))
    });
    RenderDef {
        id: j.get("id").and_then(Json::as_str).unwrap_or("").to_string(),
        condition: j.get("condition").filter(|c| **c != Json::Null).map(anim::script),
        arrays,
        geometry: j.get("geometry").map(anim::script).unwrap_or_else(|| molang::compile_or_zero("Geometry.default")),
        textures: j.get("textures").map(Json::as_arr).unwrap_or(&[]).iter().map(anim::script).collect(),
        materials: rules(j.get("materials")),
        part_visibility: rules(j.get("part_visibility")),
        color: color(j.get("color")),
        overlay_color: color(j.get("overlay_color")),
        on_fire_color: color(j.get("on_fire_color")),
        is_hurt_color: color(j.get("is_hurt_color")),
        ignore_lighting: j.get("ignore_lighting").and_then(Json::as_bool).unwrap_or(false),
        light_multiplier: j.get("light_color_multiplier").map(anim::script),
        uv_anim,
    }
}

impl Bundle {
    pub fn parse(j: &Json) -> Bundle {
        let str_of = |k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_string();
        let mut targets = crate::FastMap::default();
        for (key, v) in j.get("animations").map(Json::entries).unwrap_or(&[]) {
            let key_l = key.to_ascii_lowercase();
            if let Some(c) = v.get("controller") {
                let mut states = crate::FastMap::default();
                for (name, s) in c.get("states").map(Json::entries).unwrap_or(&[]) {
                    let mut transitions = Vec::new();
                    for t in s.get("transitions").map(Json::as_arr).unwrap_or(&[]) {
                        for (to, cond) in t.entries() {
                            transitions.push((Rc::from(to.as_str()), anim::script(cond)));
                        }
                    }
                    let particles = s
                        .get("particle_effects")
                        .map(Json::as_arr)
                        .unwrap_or(&[])
                        .iter()
                        .filter_map(|p| {
                            Some(ParticleRef {
                                effect: p.get("effect")?.as_str()?.to_string(),
                                locator: p.get("locator").and_then(Json::as_str).map(str::to_ascii_lowercase),
                                script: p.get("pre_effect_script").and_then(Json::as_str).map(molang::compile_or_zero),
                            })
                        })
                        .collect();
                    let sounds = s
                        .get("sound_effects")
                        .map(Json::as_arr)
                        .unwrap_or(&[])
                        .iter()
                        .filter_map(|p| Some(p.get("effect")?.as_str()?.to_string()))
                        .collect();
                    states.insert(
                        Rc::from(name.as_str()),
                        Rc::new(State {
                            name: name.clone(),
                            animations: entry_list(s.get("animations")),
                            transitions,
                            blend: blend(s.get("blend_transition")),
                            on_entry: scripts(s.get("on_entry")),
                            on_exit: scripts(s.get("on_exit")),
                            particles,
                            sounds,
                        }),
                    );
                }
                let initial = c.get("initial_state").and_then(Json::as_str).unwrap_or("default");
                targets.insert(
                    key_l,
                    Target::Ctrl(Rc::new(ControllerDef { key: key.clone(), initial: Rc::from(initial), states })),
                );
            } else {
                let opt = |k: &str| v.get(k).map(anim::script);
                let looping = v.get("loop").map(|l| match l {
                    Json::Bool(true) => Loop::Loop,
                    Json::Str(s) if s == "hold_on_last_frame" => Loop::HoldOnLastFrame,
                    _ => Loop::Once,
                });
                targets.insert(
                    key_l,
                    Target::Anim(Rc::new(AnimDef {
                        key: key.clone(),
                        animation: v.get("animation").and_then(Json::as_str).unwrap_or("").to_string(),
                        looping,
                        length: v.get("animation_length").and_then(Json::as_f64),
                        anim_time_update: opt("anim_time_update"),
                        blend_weight: opt("blend_weight"),
                        start_delay: opt("start_delay"),
                        loop_delay: opt("loop_delay"),
                        override_previous: v.get("override_previous_animation").and_then(Json::as_bool).unwrap_or(false),
                        relative_to_entity: v
                            .get("relative_to_entity")
                            .map(Json::as_arr)
                            .unwrap_or(&[])
                            .iter()
                            .filter_map(Json::as_str)
                            .map(|s| Rc::from(s.to_ascii_lowercase()))
                            .collect(),
                        timeline: anim::events(v.get("timeline"), ""),
                        sounds: anim::events(v.get("sound_effects"), "effect"),
                        particles: anim::events(v.get("particle_effects"), "effect"),
                    })),
                );
            }
        }
        let s = j.get("scripts");
        let sg = |k: &str| s.and_then(|s| s.get(k));
        let materials = j
            .get("materials")
            .map(Json::entries)
            .unwrap_or(&[])
            .iter()
            .map(|(k, v)| {
                (
                    k.to_ascii_lowercase(),
                    Material {
                        bedrock: v.get("bedrock").and_then(Json::as_str).unwrap_or("").to_string(),
                        render_type: v.get("render_type").and_then(Json::as_str).unwrap_or("cutout").to_string(),
                        uv_anim: v.get("uv_anim").and_then(Json::as_bool).unwrap_or(false),
                        multi_texture: v.get("multi_texture").and_then(Json::as_bool).unwrap_or(false),
                        glint: v.get("glint").and_then(Json::as_bool).unwrap_or(false),
                        bilinear: v.get("bilinear").and_then(Json::as_bool).unwrap_or(false),
                    },
                )
            })
            .collect();
        Bundle {
            id: str_of("identifier"),
            animation_file: str_of("animation_file"),
            geometry: ids(j.get("geometry")),
            textures: ids(j.get("textures")),
            materials,
            targets,
            initialize: scripts(sg("initialize")),
            pre_animation: scripts(sg("pre_animation")),
            parent_setup: scripts(sg("parent_setup")),
            animate: entry_list(sg("animate")),
            scale: sg("scale").map(anim::script),
            scale_x: sg("scaleX").map(anim::script),
            scale_y: sg("scaleY").map(anim::script),
            scale_z: sg("scaleZ").map(anim::script),
            render: j.get("render_controllers").map(Json::as_arr).unwrap_or(&[]).iter().map(render_def).collect(),
            sound_effects: ids(j.get("sound_effects")),
            particle_effects: ids(j.get("particle_effects")),
            bindings: j
                .get("bone_bindings")
                .map(Json::entries)
                .unwrap_or(&[])
                .iter()
                .map(|(g, b)| {
                    let list = b.entries().iter().filter_map(|(bone, e)| Some((bone.to_ascii_lowercase(), anim::script(e)))).collect();
                    (g.clone(), list)
                })
                .collect(),
            anims: RefCell::new(crate::FastMap::default()),
        }
    }

    /// Supplies the animations of `animation_file`.
    pub fn set_animations(&self, all: crate::FastMap<String, Animation>) {
        let mut a = self.anims.borrow_mut();
        for (k, v) in all {
            a.insert(k, Rc::new(v));
        }
    }

    /// Supplies animations already parsed (an animation file many bundles share).
    pub fn set_shared_animations(&self, all: &crate::FastMap<String, Rc<Animation>>) {
        let mut a = self.anims.borrow_mut();
        for (k, v) in all {
            a.insert(k.clone(), v.clone());
        }
    }

    pub fn animation(&self, name: &str) -> Option<Rc<Animation>> {
        self.anims.borrow().get(name).cloned()
    }
}
