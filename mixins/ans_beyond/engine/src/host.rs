// What the game glue drives: one `Host` per game thread, holding the loaded
// bundles and geometry, each mob's runtime, the pack settings, and the
// sounds waiting to be played.

use crate::assets::{Library, Source};
use crate::json::{self, Json};
use crate::molang::{Env, Expr, Scope, Value};
use crate::render::{self, Quad, RenderState};
use crate::runtime::{Effect, Runtime};


/// Actions & Beyond's pack settings (`assets/ans/entity/_settings.json`):
/// dropdowns and toggles the entity scripts read with
/// `q.is_pack_setting_selected` and `q.is_pack_setting_enabled`.
#[derive(Default, Clone, Debug)]
pub struct PackSettings {
    pub defs: Vec<SettingDef>,
    pub chosen: crate::FastMap<String, String>,
    /// Each setting's default by id (the scripts ask many times a frame).
    defaults: crate::FastMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct SettingDef {
    pub id: String,
    pub readable: String,
    pub options: Vec<String>,
    pub default: String,
    pub toggle: bool,
}

impl PackSettings {
    pub fn parse(j: &Json) -> PackSettings {
        let mut defs = Vec::new();
        for (id, v) in j.entries() {
            let toggle = v.get("type").and_then(Json::as_str) == Some("toggle");
            let options: Vec<String> =
                v.get("options").map(Json::as_arr).unwrap_or(&[]).iter().filter_map(Json::as_str).map(str::to_string).collect();
            let default = match v.get("default") {
                Some(Json::Str(s)) => s.clone(),
                Some(Json::Bool(b)) => b.to_string(),
                // A multiselect: every option in `defaults`, comma separated.
                _ if v.get("defaults").is_some() => {
                    v.get("defaults").map(Json::as_arr).unwrap_or(&[]).iter().filter_map(Json::as_str).collect::<Vec<_>>().join(",")
                }
                _ => options.first().cloned().unwrap_or_default(),
            };
            defs.push(SettingDef {
                id: id.clone(),
                readable: v.get("readable").and_then(Json::as_str).unwrap_or(id).to_string(),
                options,
                default,
                toggle,
            });
        }
        let defaults = defs.iter().map(|d| (d.id.clone(), d.default.clone())).collect();
        PackSettings { defs, chosen: Default::default(), defaults }
    }
    pub fn value(&self, id: &str) -> Option<&str> {
        if let Some(v) = self.chosen.get(id) {
            return Some(v);
        }
        self.defaults.get(id).map(String::as_str)
    }
    pub fn selected(&self, id: &str, option: &str) -> bool {
        // A multiselect holds several options, comma separated.
        self.value(id).is_some_and(|v| v == option || v.split(',').any(|o| o.trim() == option))
    }
    pub fn enabled(&self, id: &str) -> bool {
        self.value(id).is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "enabled" || v == "on")
    }
}

/// The player model's own render controllers that draw the item held in the
/// main hand (Actions & Stuff's 3D armour pieces and elytra in hand, their
/// enchanted look and trims): the game's item there is then not drawn again.
pub const HELD_MODEL_CONTROLLERS: [&str; 3] = [
    "controller.render.oreville_ans.ryfpdk",
    "controller.render.oreville_ans.ieaaxo",
    "controller.render.oreville_ans.nnryoa",
];

/// The pack's expressions setting (eyes, blinking): see `HostEnv`.
pub const EXPRESSIONS: &str = "oreville_ans:wzqdjp";

/// Queries the host answers itself, then the game's.
struct HostEnv<'a> {
    settings: &'a PackSettings,
    /// The geometry drawn last frame, for `q.bone_origin`.
    geometry: Option<std::rc::Rc<crate::geo::Geometry>>,
    /// Last frame's pose, for `q.bone_orientation_trs`.
    last_pose: &'a crate::runtime::Pose,
    game: &'a mut dyn Env,
}

impl Env for HostEnv<'_> {
    fn query(&mut self, scope: &mut Scope, name: &str, args: &[Value]) -> Value {
        match name {
            "is_pack_setting_selected" => match args {
                [Value::Str(id), Value::Str(opt)] => Value::bool(self.settings.selected(id, opt)),
                _ => Value::ZERO,
            },
            "is_pack_setting_enabled" => match args.first() {
                // Expressions only for a skin drawn for them (the jar's
                // `ExpressionSkins.allows`): the game says (`ans_expression_skin`).
                Some(Value::Str(id)) if &**id == EXPRESSIONS => {
                    Value::bool(self.settings.enabled(id) && self.game.query(scope, "ans_expression_skin", &[]).truthy())
                }
                Some(Value::Str(id)) => Value::bool(self.settings.enabled(id)),
                _ => Value::ZERO,
            },
            // Bedrock's generic queries.
            "any" => match args.split_first() {
                Some((v, rest)) => Value::bool(rest.iter().any(|x| same(x, v))),
                None => Value::ZERO,
            },
            "all" => match args.split_first() {
                Some((v, rest)) => Value::bool(rest.iter().all(|x| same(x, v))),
                None => Value::ZERO,
            },
            "in_range" => match args {
                [v, lo, hi, ..] => Value::bool(v.num() >= lo.num() && v.num() <= hi.num()),
                _ => Value::ZERO,
            },
            "approx_eq" => match args.split_first() {
                Some((v, rest)) => Value::bool(rest.iter().all(|x| (x.num() - v.num()).abs() < 1e-6)),
                None => Value::ZERO,
            },
            "count" => Value::Num(args.len() as f32),
            // `BoneTransforms.origin`: the pivot, with y from the model's top.
            "bone_origin" => {
                let p = args
                    .first()
                    .and_then(Value::as_str)
                    .and_then(|b| { let g = self.geometry.as_ref()?; Some(g.bones[g.bone(b)?].pivot) })
                    .unwrap_or([0.0; 3]);
                Value::Vec3([p[0], 24.0 - p[1], p[2]])
            }
            // The bone's pivot (y from the model's top, as `bone_origin`) moved by its
            // animated position, its rest plus animated rotation (degrees) and its
            // scale, as posed last frame.
            "bone_orientation_trs" => {
                let Some(name) = args.first().and_then(Value::as_str) else { return Value::ZERO };
                let Some((pivot, rest)) =
                    self.geometry.as_ref().and_then(|g| { let b = &g.bones[g.bone(name)?]; Some((b.pivot, b.rotation)) })
                else {
                    return Value::Trs([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
                };
                let p = self.last_pose.get(name);
                let f = |v: f32| if v.is_finite() { v } else { 0.0 };
                Value::Trs([
                    pivot[0] + f(p[3]), 24.0 - pivot[1] + f(p[4]), pivot[2] + f(p[5]),
                    rest[0] + f(p[0]), rest[1] + f(p[1]), rest[2] + f(p[2]),
                    p[6], p[7], p[8],
                ])
            }
            _ => self.game.query(scope, name, args),
        }
    }
    fn arrow(&mut self, target: u32, rhs: &Expr, scope: &mut Scope) -> Value {
        self.game.arrow(target, rhs, scope)
    }
}

/// Molang `==` between two values: strings by text, a string never equals a number.
fn same(a: &Value, b: &Value) -> bool {
    match (a.as_str(), b.as_str()) {
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(y),
        (None, None) => a.num() == b.num(),
        _ => false,
    }
}

struct MobState {
    rt: Runtime,
    geometry: Option<std::rc::Rc<crate::geo::Geometry>>,
    last_age: f64,
    last_frame: u64,
    /// The held items' attachables (main hand, off hand): the item and its runtime.
    att: [Option<(String, Runtime)>; 6],
}

/// The item slots attachables are drawn for, as `c.item_slot` names them.
const SLOTS: [&str; 6] = ["main_hand", "off_hand", "head", "chest", "legs", "feet"];

/// Queries an attachable asks: its own, then its holder's (Bedrock answers an
/// attachable's `q.*` about the entity holding it).
struct AttEnv<'a, 'b> {
    inner: &'a mut HostEnv<'b>,
    owner: u32,
    owner_kind: &'a str,
}

impl Env for AttEnv<'_, '_> {
    fn query(&mut self, scope: &mut Scope, name: &str, args: &[Value]) -> Value {
        match name {
            // Attachables.slotBone for a player (MobEquipment's default bones).
            "item_slot_to_bone_name" => Value::str(match args.first().and_then(Value::as_str).unwrap_or("") {
                "main_hand" => "rightitem",
                "off_hand" => "leftitem",
                "head" => "head",
                "chest" | "torso" => "body",
                "legs" => "waist",
                "feet" => "rightleg",
                other => other,
            }),
            "is_owner_identifier_any" => {
                Value::bool(args.iter().filter_map(Value::as_str).any(|a| a.eq_ignore_ascii_case(self.owner_kind)))
            }
            "is_attached" | "is_attached_to_entity" => Value::bool(true),
            _ => self.inner.query(scope, name, args),
        }
    }
    fn arrow(&mut self, target: u32, rhs: &crate::molang::Expr, scope: &mut Scope) -> Value {
        // `c.owning_entity->v.x`: the holder's variables are this scope's.
        if target == self.owner {
            scope.eval_expr(rhs, self)
        } else {
            self.inner.arrow(target, rhs, scope)
        }
    }
}

/// A sound to play where a mob is: the resolved sound event id.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub mob: u32,
    pub event: String,
}

#[derive(Default)]
pub struct Host {
    pub lib: Library,
    pub settings: PackSettings,
    settings_loaded: bool,
    /// `sound_meta.json`: Bedrock sound event names to the content mod's.
    sound_ids: crate::FastMap<String, String>,
    mobs: crate::FastMap<u32, MobState>,
    pub sounds: Vec<Sound>,
    frame: u64,
    pub quads: Vec<Quad>,
    /// The last `draw_mob`'s hand item frames (right, left) in Horizon's mob
    /// frame: blocks, a proper rotation, origin at the `rightitem` / `leftitem`
    /// bone's pivot. None when the model has no such bone.
    pub hands: [Option<render::Mat4>; 2],
    /// Held items drawn without the jar's attachables (`set_attachables(false)`).
    no_attachables: bool,
    att_info: [String; 6],
    /// Textures taller than the game's atlas takes (512 rows), shipped cut in
    /// 512-row parts `<name>__p<i>`: sprite name to its height in pixels.
    split: crate::FastMap<String, f32>,
    /// Textures the pack ships an opaque copy of (`opaque_textures.json`).
    opaque: std::collections::HashSet<String>,
    /// The texture the jar composes at run time (`atlas_cells.json`): its size,
    /// cell size, and which vanilla sprite fills each cell.
    atlas_cells: crate::FastMap<String, ([f32; 2], f32, crate::FastMap<(u32, u32), std::rc::Rc<str>>)>,
    /// Textures the jar composes from the game's (`composites.json`), by sprite.
    composites: crate::FastMap<String, render::Composite>,
    /// Pixel sizes of the game's sprites the composites copy from (the game
    /// fills them: `composite_sprites`, `sprite_sizes`).
    pub sprite_sizes: crate::FastMap<String, [f32; 2]>,
    /// The pose buffer a mob's runtime ticks into next (`draw_mob`).
    spare_pose: crate::runtime::Pose,
    /// The next `draw_mob`'s change colour (0xRRGGBB; 0 none): a sheep's wool,
    /// a cat's collar (the jar's `AnimatedMob.changeColor`).
    pub change_color: u32,
    /// The quads (start, end) each held attachable added in the last `draw_mob`.
    pub att_quads: [(usize, usize); 6],
    /// The quads (start, end) of the held item the mob's own model drew in
    /// the main hand in the last `draw_mob` (`HELD_MODEL_CONTROLLERS`: the
    /// player's armour piece or elytra held in hand); empty when none.
    pub held_model: (usize, usize),
    /// The quads (start, end) of each box the mob's own model drew in the
    /// last `draw_mob` (not its attachables'): the first person view leaves
    /// out one the camera is in.
    pub cube_spans: Vec<(usize, usize)>,
    /// The last `draw_mob`'s first layer: its geometry, posed bones and scale,
    /// for `armor`.
    last: Option<(std::rc::Rc<crate::geo::Geometry>, Vec<render::Mat4>, [f32; 3])>,
    /// Per frame cost in seconds of the last `draw_mob` calls, for the
    /// benchmark overlay.
    pub spent: f64,
}

/// What the game tells the host about a mob each frame.
pub struct MobFrame<'a> {
    pub id: u32,
    pub kind: &'a str,
    /// Ticks lived plus the partial tick.
    pub age_ticks: f64,
    pub hurt: bool,
    pub on_fire: bool,
    /// A player's skin sprite, drawn where the bundle's default texture is.
    pub skin: Option<&'a str>,
    /// Variables the engine sets before the scripts run (`v.is_first_person`,
    /// `v.attack_time`...), as A&B's EngineVariables does.
    pub vars: &'a [(&'a str, f32)],
    /// Context variables (`c.player_arm_height`).
    pub context: &'a [(&'a str, f32)],
    /// The items in its hands (main, off; "" for none), drawn with the
    /// jar's attachables when it has them.
    pub held: [&'a str; 2],
    /// What it wears (head, chest, legs, feet; "" for none) that should be
    /// drawn with the jar's attachables (the elytra's).
    pub worn: [&'a str; 4],
}

impl Host {
    pub fn load_settings(&mut self, src: &dyn Source) {
        if self.settings_loaded {
            return;
        }
        self.settings_loaded = true;
        if let Some(j) = src.read("assets/ans/sound_meta.json").and_then(|b| json::parse(std::str::from_utf8(&b).ok()?).ok()) {
            for (k, v) in j.get("events").map(Json::entries).unwrap_or(&[]) {
                if let Some(id) = v.get("id").and_then(Json::as_str) {
                    self.sound_ids.insert(k.to_ascii_lowercase(), id.to_string());
                }
            }
        }
        if let Some(j) = src.read("assets/ans/composites.json").and_then(|b| json::parse(std::str::from_utf8(&b).ok()?).ok()) {
            for (k, v) in j.entries() {
                let nums = |x: Option<&json::Json>| x.map(|a| a.as_arr().iter().filter_map(|n| n.as_f64()).map(|n| n as f32).collect::<Vec<f32>>()).unwrap_or_default();
                let size = nums(v.get("size"));
                let parts = v
                    .get("parts")
                    .map(|p| p.as_arr())
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|p| {
                        let src = nums(p.get("src"));
                        let dst = nums(p.get("dst"));
                        Some(render::CompositePart {
                            sprite: std::rc::Rc::from(p.get("sprite")?.as_str()?),
                            src: (src.len() == 4).then(|| [src[0], src[1], src[2], src[3]]),
                            dst: [*dst.first()?, *dst.get(1)?],
                            tinted: p.get("tinted").and_then(|t| t.as_bool()).unwrap_or(false),
                        })
                    })
                    .collect();
                if size.len() == 2 {
                    self.composites.insert(k.clone(), render::Composite { size: [size[0], size[1]], parts });
                }
            }
        }
        if let Some(j) = src.read("assets/ans/atlas_cells.json").and_then(|b| json::parse(std::str::from_utf8(&b).ok()?).ok()) {
            for (k, v) in j.entries() {
                let n = |key: &str| v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                let cell = n("cell").max(1.0);
                let mut map = crate::FastMap::default();
                for c in v.get("cells").map(|c| c.as_arr()).unwrap_or(&[]) {
                    let c = c.as_arr();
                    if let (Some(x), Some(y), Some(t)) = (c.first().and_then(|x| x.as_f64()), c.get(1).and_then(|x| x.as_f64()), c.get(2).and_then(|x| x.as_str())) {
                        map.insert((x as u32, y as u32), std::rc::Rc::from(t));
                    }
                }
                self.atlas_cells.insert(k.clone(), ([n("w"), n("h")], cell, map));
            }
        }
        if let Some(j) = src.read("assets/ans/opaque_textures.json").and_then(|b| json::parse(std::str::from_utf8(&b).ok()?).ok()) {
            self.opaque = j.as_arr().iter().filter_map(|v| v.as_str()).map(str::to_string).collect();
        }
        if let Some(j) = src.read("assets/ans/split_textures.json").and_then(|b| json::parse(std::str::from_utf8(&b).ok()?).ok()) {
            for (k, v) in j.entries() {
                if let Some(h) = v.as_f64() {
                    self.split.insert(k.clone(), h as f32);
                }
            }
        }
        if let Some(j) = src.read("assets/ans/entity/_settings.json").and_then(|b| json::parse(std::str::from_utf8(&b).ok()?).ok()) {
            let chosen = std::mem::take(&mut self.settings.chosen);
            self.settings = PackSettings::parse(&j);
            self.settings.chosen = chosen;
        }
    }

    /// Whether there is a bundle for entity type `kind`.
    pub fn handles(&mut self, src: &dyn Source, kind: &str) -> bool {
        self.lib.bundle(src, kind).is_some()
    }

    /// Call once a frame before the mobs are drawn: forgets mobs not drawn
    /// for a while.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        let now = self.frame;
        // A mob out of view for 10 s (at 60 fps) starts over when seen again.
        self.mobs.retain(|_, m| now - m.last_frame < 600);
    }

    /// Animates mob `m` and leaves its quads (in Horizon's mob frame) in
    /// `self.quads`. False when there is no bundle for it.
    pub fn draw_mob(&mut self, src: &dyn Source, m: &MobFrame, env: &mut dyn Env) -> bool {
        self.att_info = Default::default();
        self.att_quads = Default::default();
        self.held_model = (0, 0);
        self.cube_spans.clear();
        let started = std::time::Instant::now();
        self.quads.clear();
        self.hands = [None, None];
        self.last = None;
        self.load_settings(src);
        let Some(bundle) = self.lib.bundle(src, m.kind) else { return false };
        let frame = self.frame;
        let st = self.mobs.entry(m.id).or_insert_with(|| MobState {
            rt: Runtime::new(bundle.clone(), 0x9E37_79B9_7F4A_7C15 ^ m.id as u64),
            geometry: None,
            last_age: m.age_ticks,
            last_frame: frame,
            att: Default::default(),
        });
        // Drawn twice in a frame (a shadow pass, a reflection): no time passes.
        let dt = if st.last_frame == frame && st.last_age == m.age_ticks {
            0.0
        } else {
            ((m.age_ticks - st.last_age) / 20.0).clamp(0.0, 0.25)
        };
        st.last_age = m.age_ticks;
        st.last_frame = frame;
        if st.geometry.is_none() {
            st.geometry = bundle.geometry.get("default").and_then(|g| self.lib.geometry(src, g));
        }
        for (k, v) in m.vars {
            st.rt.scope.set_var(k, Value::Num(*v));
        }
        for (k, v) in m.context {
            st.rt.scope.context.insert(crate::molang::intern(k), Value::Num(*v));
        }
        // Last frame's pose, for the scripts' bone queries: moved out, the
        // runtime ticking into a spare one (cloning it cost every draw).
        let last_pose = std::mem::replace(&mut st.rt.pose, std::mem::take(&mut self.spare_pose));
        let mut env = HostEnv { settings: &self.settings, geometry: st.geometry.clone(), last_pose: &last_pose, game: env };
        st.rt.tick(dt, m.age_ticks / 20.0, &mut env);
        // The held items' attachables run after their holder, on its variables
        // (they set what its render controllers read: `v.oreville_mainhand_visible`).
        let first_person = st.rt.scope.var("is_first_person").map_or(0.0, Value::num);
        for (slot, item) in m.held.iter().chain(m.worn.iter()).enumerate() {
            let want = if item.is_empty() || self.no_attachables { None } else { Some(*item) };
            let keep = matches!((&st.att[slot], want), (Some((i, _)), Some(w)) if i == w);
            if !keep {
                st.att[slot] = want.and_then(|item| {
                    let path = self.lib.attachables_for(src, item).first().map(|(p, _)| p.clone())?;
                    let b = self.lib.attachable(src, &path)?;
                    // In a hand, an attachable that binds none of its bones (an
                    // armour piece's or the elytra's, made to be worn) has nothing
                    // to hold it by: the game draws that item instead.
                    if slot < 2 && b.bindings.values().all(Vec::is_empty) {
                        return None;
                    }
                    Some((item.to_string(), Runtime::new(b, 0x51ED_270B ^ m.id as u64 ^ slot as u64)))
                });
            }
            let Some((_, rt)) = &mut st.att[slot] else { continue };
            std::mem::swap(&mut rt.scope.vars, &mut st.rt.scope.vars);
            let c = &mut rt.scope.context;
            c.insert(crate::molang::intern("item_slot"), Value::str(SLOTS[slot]));
            c.insert(crate::molang::intern("is_first_person"), Value::Num(first_person));
            c.insert(crate::molang::intern("owning_entity"), Value::Actor(m.id));
            let mut aenv = AttEnv { inner: &mut env, owner: m.id, owner_kind: m.kind };
            let ab = rt.bundle.clone();
            for s in &ab.parent_setup {
                rt.scope.run(s, &mut aenv);
            }
            rt.tick(dt, m.age_ticks / 20.0, &mut aenv);
            std::mem::swap(&mut rt.scope.vars, &mut st.rt.scope.vars);
        }
        let scale = st.rt.scale(&mut env);
        for e in &st.rt.effects {
            if let Effect::Sound { name, .. } = e {
                if let Some(ev) = bundle.sound_effects.get(&name.to_ascii_lowercase()) {
                    let event = self.sound_ids.get(ev).cloned().unwrap_or_else(|| java_sound(ev));
                    self.sounds.push(Sound { mob: m.id, event });
                }
            }
        }
        let rs = RenderState { hurt: m.hurt, on_fire: m.on_fire };
        let lib = &mut self.lib;
        let layers = render::layers(&bundle, &mut st.rt.scope, &mut env, &rs, &mut |id| lib.geometry(src, id));
        if crate::debug() {
            eprintln!("vars fp={:?} ezz={:?}", st.rt.scope.var("is_first_person"), st.rt.scope.var("ezzhwg"));
            if let Ok(list) = std::env::var("ANS_TRACE_VARS") {
                let v: Vec<String> = list.split(',').map(|n| format!("{n}={:?}", st.rt.scope.var(n))).collect();
                eprintln!("trace {}", v.join(" "));
            }
            for l in &layers {
                eprintln!("layer {} {} visible {}/{} kinds {:?}", l.geometry, l.texture, l.visible.iter().filter(|v| **v).count(), l.visible.len(),
                    l.kinds.iter().filter(|k| **k != render::Kind::Cutout).count());
            }
        }
        let default_tex = bundle.textures.get("default").cloned();
        let mut layers = layers;
        if let (Some(skin), Some(def)) = (m.skin, default_tex) {
            for l in &mut layers {
                if l.texture == def {
                    l.texture = format!("minecraft:textures/{skin}.png");
                }
            }
        }
        let mut drawn_geo: Vec<&str> = Vec::new();
        let mut posed = false;
        let mut drawn_model = false;
        for l in &layers {
            if let Some(g) = lib.geometry(src, &l.geometry) {
                let bones = render::bone_matrices(&g, &st.rt.pose, &st.rt.entity_relative);
                // The hands and pose of the model's first layer.
                if !posed {
                    posed = true;
                    self.hands = render::hand_frames(&g, &bones, scale);
                    self.last = Some((g.clone(), bones.clone(), scale));
                    st.geometry = Some(g.clone());
                }
                // Each layer over the same geometry a little further out.
                let under = drawn_geo.iter().filter(|x| **x == l.geometry).count();
                let before = self.quads.len();
                render::mesh_cubes(&g, l, &bones, scale, under as f32 * 0.002, &mut self.quads, &mut self.cube_spans);
                // Only a layer that drew something lifts the next one.
                if self.quads.len() == before {
                    continue;
                }
                // The hands and pose (and where armour goes) are the model's
                // that is drawn: a morph's own, not the player's under it.
                if !drawn_model {
                    drawn_model = true;
                    self.hands = render::hand_frames(&g, &bones, scale);
                    self.last = Some((g.clone(), bones.clone(), scale));
                    st.geometry = Some(g.clone());
                }
                if HELD_MODEL_CONTROLLERS.iter().any(|c| l.controller == *c || l.controller.starts_with(&format!("{c}."))) {
                    self.held_model = if self.held_model.1 > self.held_model.0 {
                        (self.held_model.0.min(before), self.quads.len())
                    } else {
                        (before, self.quads.len())
                    };
                }
                drawn_geo.push(&l.geometry);
            }
        }
        // The attachables, their bound bones following the holder's (`bone_bindings`).
        if let Some((hg, hbones, _)) = self.last.clone() {
            for slot in 0..SLOTS.len() {
                let Some((_, rt)) = &mut st.att[slot] else { continue };
                let ab = rt.bundle.clone();
                std::mem::swap(&mut rt.scope.vars, &mut st.rt.scope.vars);
                let mut aenv = AttEnv { inner: &mut env, owner: m.id, owner_kind: m.kind };
                let alayers = render::layers(&ab, &mut rt.scope, &mut aenv, &rs, &mut |id| lib.geometry(src, id));
                let bound_for = |gid: &str, g: &crate::geo::Geometry, scope: &mut Scope, aenv: &mut AttEnv| {
                    let mut out = Vec::new();
                    let bindings = ab.bindings.get(gid).map(Vec::as_slice).unwrap_or(&[]);
                    // No bone_bindings (the elytra's): its bones follow the holder's
                    // bones of the same name, as AttachableRenderer does.
                    let by_name: Vec<(usize, usize)> = if bindings.is_empty() && slot >= 2 {
                        g.bones.iter().enumerate().filter_map(|(ai, b)| Some((ai, hg.bone(&b.name)?))).collect()
                    } else {
                        Vec::new()
                    };
                    let bound_pairs = bindings
                        .iter()
                        .filter_map(|(bone, expr)| {
                            let name = scope.run(expr, aenv);
                            Some((g.bone(bone)?, name.as_str().and_then(|n| hg.bone(n))?))
                        })
                        .collect::<Vec<_>>();
                    for (ai, hi) in bound_pairs.into_iter().chain(by_name) {
                        // The jar's AttachableRenderer.anchors: the holder bone's matrix
                        // at its pivot (holderMatrix), then anchorOffset.
                        // A bound bone with a parent uses that parent's pivot; one without
                        // is set 1.5 blocks below the holder's pivot (translate(0, -1.5, 0)).
                        let hp = hg.bones[hi].pivot;
                        let ap = g.bones[ai].parent.map_or([0.0, 24.0, 0.0], |p| g.bones[p].pivot);
                        let off = [hp[0] - ap[0], hp[1] - ap[1], hp[2] - ap[2]];
                        out.push((ai, hbones[hi].mul(&render::Mat4::translate(off))));
                    }
                    out
                };
                let mut info = format!("{}:", SLOTS[slot]);
                let first_quad = self.quads.len();
                // Each layer over the same geometry a little further out, as
                // the holder's: a held model's overlays (its enchanted look,
                // its glow) would otherwise fight its base for depth.
                let mut drawn: Vec<String> = Vec::new();
                for l in &alayers {
                    if let Some(g) = lib.geometry(src, &l.geometry) {
                        let bound = bound_for(&l.geometry, &g, &mut rt.scope, &mut aenv);
                        let before = self.quads.len();
                        if crate::debug() {
                            eprintln!("attachable layer {} {} bound {:?}", l.geometry, l.texture, bound.iter().map(|b| b.0).collect::<Vec<_>>());
                        }
                        let bones = render::bone_matrices_bound(&g, &rt.pose, &rt.entity_relative, &bound);
                        let under = drawn.iter().filter(|x| **x == l.geometry).count();
                        render::mesh(&g, l, &bones, scale, under as f32 * 0.002, &mut self.quads);
                        drawn.push(l.geometry.clone());
                        let names: Vec<&str> = bound.iter().map(|b| g.bones[b.0].name.as_str()).collect();
                        let first = self.quads.get(before).map(|q| q.corners[0]);
                        info.push_str(&format!(" [{} {} bound={:?} quads={} at={:?}]", l.geometry, l.texture, names, self.quads.len() - before, first));
                    } else {
                        info.push_str(&format!(" [{} missing]", l.geometry));
                    }
                }
                self.att_info[slot] = info;
                self.att_quads[slot] = (first_quad, self.quads.len());
                std::mem::swap(&mut rt.scope.vars, &mut st.rt.scope.vars);
            }
        }
        drop(env);
        self.spare_pose = last_pose;
        if !self.composites.is_empty() && self.quads.iter().any(|q| self.composites.contains_key(&*q.sprite)) {
            // One quad a part: the attachables' quads move along.
            let old = std::mem::take(&mut self.quads);
            let mut index = Vec::with_capacity(old.len() + 1);
            for q in &old {
                index.push(self.quads.len());
                match self.composites.get(&*q.sprite) {
                    Some(c) => render::composite_parts(q, c, &self.sprite_sizes, self.change_color, &mut self.quads),
                    None => self.quads.push(q.clone()),
                }
            }
            index.push(self.quads.len());
            for r in self.att_quads.iter_mut().chain(std::iter::once(&mut self.held_model)).chain(self.cube_spans.iter_mut()) {
                *r = (index[r.0.min(old.len())], index[r.1.min(old.len())]);
            }
        }
        if !self.atlas_cells.is_empty() {
            for q in &mut self.quads {
                let base = q.sprite.strip_suffix(render::LIT).or_else(|| q.sprite.strip_suffix(render::GLOW)).unwrap_or(&q.sprite);
                if let Some((size, cell, cells)) = self.atlas_cells.get(base) {
                    if let Some((sprite, uv)) = render::atlas_cell(q.uv, *size, *cell, cells) {
                        // The game's own sprites have no glowing half.
                        let glow = q.sprite.ends_with(render::GLOW);
                        q.sprite = if glow { std::rc::Rc::from("") } else { sprite };
                        q.full_bright &= !glow;
                        q.uv = uv;
                    }
                }
            }
        }
        for q in &mut self.quads {
            // No halves shipped (no texel glows): the texture as it is, lit;
            // its glowing half draws nothing (an empty sprite is skipped).
            let half = q.sprite.strip_suffix(render::LIT).map(|b| (b, false)).or_else(|| q.sprite.strip_suffix(render::GLOW).map(|b| (b, true)));
            if let Some((base, glow)) = half {
                if !self.opaque.contains(base) {
                    q.sprite = if glow { std::rc::Rc::from("") } else { base.into() };
                    q.full_bright = false;
                }
            }
        }
        if !self.split.is_empty() {
            for q in &mut self.quads {
                if let Some(&h) = self.split.get(&*q.sprite) {
                    render::to_split_part(q, h);
                }
            }
        }
        self.spent += started.elapsed().as_secs_f64();
        true
    }

    /// `name=value` for each of `names` in mob `id`'s variables, for a trace.
    pub fn var_trace(&self, id: u32, names: &[&str]) -> String {
        let Some(st) = self.mobs.get(&id) else { return String::from("(not drawn)") };
        names.iter().map(|n| format!("{n}={:?}", st.rt.scope.var(n).map(Value::num))).collect::<Vec<_>>().join(" ")
    }

    /// What the last `draw_mob` drew for each held attachable, for a trace.
    pub fn att_trace(&self) -> String {
        self.att_info.join(" | ")
    }

    /// Turns the jar's held-item attachables on or off.
    pub fn set_attachables(&mut self, on: bool) {
        self.no_attachables = !on;
    }
    /// Whether the last `draw_mob` drew an attachable in hand `slot`
    /// (0 main, 1 off): the game should not draw that item itself.
    pub fn drew_attachable(&self, id: u32, slot: usize) -> bool {
        let (a, b) = self.att_quads[slot];
        b > a && self.mobs.get(&id).is_some_and(|m| m.att[slot].is_some())
    }

    /// Drops what the held attachable in `slot` drew (its quads), so the game
    /// can draw that item itself: used when the game can't draw them (a
    /// texture it doesn't have).
    pub fn drop_attachable(&mut self, slot: usize) {
        let (a, b) = self.att_quads[slot];
        if b > a {
            self.quads.drain(a..b);
            for r in &mut self.att_quads {
                if r.0 >= b {
                    *r = (r.0 - (b - a), r.1 - (b - a));
                }
            }
            self.att_quads[slot] = (a, a);
        }
    }

    /// Whether every sprite the composed textures copy from has its size
    /// (`sprite_sizes`): then the game need not look again.
    pub fn composite_sizes_known(&self) -> bool {
        self.composites.values().all(|c| c.parts.iter().all(|p| self.sprite_sizes.contains_key(&*p.sprite)))
    }

    /// The game's sprites the composed textures copy from, whose pixel sizes
    /// `sprite_sizes` needs.
    pub fn composite_sprites(&self) -> Vec<String> {
        let mut v: Vec<String> = self.composites.values().flat_map(|c| c.parts.iter().map(|p| p.sprite.to_string())).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Scales what the held attachable in hand `slot` drew by `k` about that
    /// hand's item bone (`rightitem`/`leftitem`), so a first person item can
    /// be made smaller without moving it out of the hand.
    pub fn scale_attachable(&mut self, slot: usize, k: f32) {
        let (a, b) = self.att_quads[slot.min(1)];
        if b <= a || (k - 1.0).abs() < 1e-4 {
            return;
        }
        let Some(h) = self.hands.get(slot).copied().flatten() else { return };
        let o = h.point([0.0; 3]);
        for q in &mut self.quads[a..b] {
            for c in &mut q.corners {
                for i in 0..3 {
                    c[i] = o[i] + (c[i] - o[i]) * k;
                }
            }
        }
    }

    /// Adds worn armour to `quads`, posed as the last `draw_mob`'s model.
    pub fn armor(&mut self, pieces: &[render::ArmorPiece]) {
        if let Some((g, bones, scale)) = &self.last {
            render::armor_quads(g, bones, *scale, pieces, &mut self.quads);
        }
    }

    pub fn tracked(&self) -> usize {
        self.mobs.len()
    }
}

/// A Bedrock vanilla sound event (`mob.zombie.step`) as Java's
/// (`minecraft:entity.zombie.step`).
pub fn java_sound(bedrock: &str) -> String {
    if bedrock.contains(':') {
        return bedrock.to_string();
    }
    match bedrock.strip_prefix("mob.") {
        Some(rest) => format!("minecraft:entity.{rest}"),
        None => format!("minecraft:{bedrock}"),
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    #[test]
    fn any_compares_strings_as_text() {
        assert!(!same(&Value::str(""), &Value::str("bkmptz")));
        assert!(same(&Value::str("bkmptz"), &Value::str("bkmptz")));
        assert!(!same(&Value::Num(0.0), &Value::str("a")));
        assert!(same(&Value::Num(2.0), &Value::Num(2.0)));
    }
    #[test]
    fn multiselect_defaults_are_all_selected() {
        let j = crate::json::parse(r#"{"a:w":{"type":"multiselect","defaults":["x","y"],"options":["x","y","z"]}}"#).unwrap();
        let s = PackSettings::parse(&j);
        assert!(s.selected("a:w", "x") && s.selected("a:w", "y") && !s.selected("a:w", "z"));
    }
}
