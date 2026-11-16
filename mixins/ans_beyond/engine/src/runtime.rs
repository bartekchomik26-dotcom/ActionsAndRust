// An entity's animation state, as Actions & Beyond's `EntityRuntime` keeps
// it: the entity's scripts (initialize once, pre_animation every frame),
// the `animate` list, animation players and animation controllers with
// their state blending, timelines, and the bone pose they add up to.
//
// Reverse engineered from `EntityRuntime$AnimationPlayer.tick/contribute`,
// `EntityRuntime$ControllerPlayer.tick`, `EntityRuntime$StateRun.tick`,
// `EntityRuntime.tickEntries/sample` and `EntityBundle$Blend.outgoing`.

use crate::anim::{Animation, Loop};
use crate::bundle::{AnimDef, Blend, Bundle, ControllerDef, Entry, State, Target};
use crate::molang::{Env, Scope, Script, Value};

use std::rc::Rc;

/// A bone's pose: rotation (Bedrock degrees), position (pixels), scale.
pub type BonePose = [f32; 9];
pub const REST: BonePose = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0];

#[derive(Default, Debug, Clone)]
pub struct Pose {
    pub bones: crate::FastMap<Rc<str>, BonePose>,
}

impl Pose {
    pub fn bone(&mut self, name: &str) -> &mut BonePose {
        if !self.bones.contains_key(name) {
            self.bones.insert(Rc::from(name), REST);
        }
        self.bones.get_mut(name).unwrap()
    }
    pub fn get(&self, name: &str) -> BonePose {
        self.bones.get(name).copied().unwrap_or(REST)
    }
}

/// What the runtime tells the game to do this frame, besides the pose.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A sound effect by the entity's short name, from a locator or not.
    Sound { name: String, locator: Option<String> },
    /// Start a particle effect (a state's or an animation's).
    Particle { name: String, locator: Option<String>, state: Option<u64>, bound: bool },
    /// Stop a state's looping particles.
    StopParticles { state: u64 },
}

enum Player {
    Anim(AnimPlayer),
    Ctrl(Box<CtrlPlayer>),
    Missing,
}

struct AnimPlayer {
    def: Rc<AnimDef>,
    anim: Option<Rc<Animation>>,
    time: f64,
    prev: f64,
    finished: bool,
    started: bool,
    start_delay: f64,
    loop_wait: f64,
}

struct EntryRun {
    weight: Option<Rc<Script>>,
    player: Player,
}

struct StateRun {
    id: u64,
    def: Rc<State>,
    entered: f64,
    entries: Vec<EntryRun>,
}

struct Fade {
    run: StateRun,
    blend: Blend,
    start: f64,
}

struct CtrlPlayer {
    def: Rc<ControllerDef>,
    depth: u32,
    current: Option<StateRun>,
    previous: Option<StateRun>,
    fade: Blend,
    fade_start: f64,
    older: Vec<Fade>,
}

/// Queries that read the runtime itself rather than the entity.
#[derive(Default, Clone, Copy, Debug)]
pub struct Clock {
    pub anim_time: f64,
    pub state_time: f64,
    pub any_finished: bool,
    pub all_finished: bool,
    pub delta: f64,
    pub life: f64,
}

pub struct Runtime {
    pub bundle: Rc<Bundle>,
    pub scope: Scope,
    pub pose: Pose,
    pub effects: Vec<Effect>,
    pub clock: Clock,
    seconds: f64,
    initialized: bool,
    animate: Vec<EntryRun>,
    next_state: u64,
    /// Bones a contributing animation marked `relative_to_entity`.
    pub entity_relative: Vec<Rc<str>>,
}

const MAX_DEPTH: u32 = 8;

/// The game half of the runtime: queries, plus `->` targets.
pub trait Game: Env {
    fn clock(&mut self) -> &mut Clock;
}

impl Runtime {
    pub fn new(bundle: Rc<Bundle>, seed: u64) -> Runtime {
        Runtime {
            bundle,
            scope: Scope::new(seed),
            pose: Pose::default(),
            effects: Vec::new(),
            clock: Clock::default(),
            seconds: 0.0,
            initialized: false,
            animate: Vec::new(),
            next_state: 1,
            entity_relative: Vec::new(),
        }
    }

    /// One frame: `delta` seconds since the last, `life` the entity's age
    /// in seconds. `env` answers queries (it may read `self.clock`).
    pub fn tick(&mut self, delta: f64, life: f64, env: &mut dyn Env) {
        let bundle = self.bundle.clone();
        self.clock.delta = delta;
        self.clock.life = life;
        self.seconds += delta;
        self.effects.clear();
        self.pose.bones.clear();
        self.entity_relative.clear();
        let mut ctx = Ctx { rt_clock: &mut self.clock, env };
        if !self.initialized {
            self.initialized = true;
            for s in &bundle.initialize {
                self.scope.run(s, &mut ctx);
            }
            let mut next = self.next_state;
            self.animate = entries(&bundle, &bundle.animate, 0, &mut next);
            self.next_state = next;
        }
        for s in &bundle.pre_animation {
            self.scope.run(s, &mut ctx);
        }
        let mut w = World {
            bundle: &bundle,
            scope: &mut self.scope,
            pose: &mut self.pose,
            effects: &mut self.effects,
            relative: &mut self.entity_relative,
            seconds: self.seconds,
            next_state: &mut self.next_state,
        };
        tick_entries(&mut w, &mut self.animate, 1.0, &mut ctx);
    }

    /// The model scale from `scripts.scale` (and scaleX/Y/Z).
    pub fn scale(&mut self, env: &mut dyn Env) -> [f32; 3] {
        let b = self.bundle.clone();
        let mut ctx = Ctx { rt_clock: &mut self.clock, env };
        let s = b.scale.as_ref().map(|s| self.scope.run(s, &mut ctx).num()).unwrap_or(1.0);
        let mut one = |x: &Option<Script>| x.as_ref().map(|x| self.scope.run(x, &mut ctx).num()).unwrap_or(1.0);
        [s * one(&b.scale_x), s * one(&b.scale_y), s * one(&b.scale_z)]
    }
}

/// Answers the runtime's own queries (anim_time, state_time,
/// any_animation_finished...) and hands the rest to the game.
pub struct Ctx<'a> {
    pub rt_clock: &'a mut Clock,
    pub env: &'a mut dyn Env,
}

impl Env for Ctx<'_> {
    fn query(&mut self, scope: &mut Scope, name: &str, args: &[Value]) -> Value {
        let c = &self.rt_clock;
        match name {
            "anim_time" => Value::Num(c.anim_time as f32),
            "state_time" => Value::Num(c.state_time as f32),
            "any_animation_finished" => Value::bool(c.any_finished),
            "all_animations_finished" => Value::bool(c.all_finished),
            "delta_time" => Value::Num(c.delta as f32),
            "life_time" => Value::Num(c.life as f32),
            _ => self.env.query(scope, name, args),
        }
    }
    fn arrow(&mut self, target: u32, rhs: &crate::molang::Expr, scope: &mut Scope) -> Value {
        self.env.arrow(target, rhs, scope)
    }
}

struct World<'a> {
    bundle: &'a Bundle,
    scope: &'a mut Scope,
    pose: &'a mut Pose,
    effects: &'a mut Vec<Effect>,
    relative: &'a mut Vec<Rc<str>>,
    seconds: f64,
    next_state: &'a mut u64,
}

fn entries(b: &Bundle, list: &[Entry], depth: u32, next: &mut u64) -> Vec<EntryRun> {
    list.iter()
        .map(|e| EntryRun {
            weight: e.weight.clone(),
            player: match b.targets.get(&*e.target) {
                Some(Target::Anim(a)) => Player::Anim(AnimPlayer {
                    def: a.clone(),
                    anim: b.animation(&a.animation),
                    time: 0.0,
                    prev: -1e-9,
                    finished: false,
                    started: false,
                    start_delay: 0.0,
                    loop_wait: 0.0,
                }),
                Some(Target::Ctrl(c)) if depth < MAX_DEPTH => {
                    let mut p = CtrlPlayer {
                        def: c.clone(),
                        depth: depth + 1,
                        current: None,
                        previous: None,
                        fade: Blend::NONE,
                        fade_start: 0.0,
                        older: Vec::new(),
                    };
                    if let Some(s) = c.states.get(&*c.initial).or_else(|| c.states.values().next()) {
                        p.current = Some(new_state(b, s.clone(), depth + 1, next, 0.0));
                    }
                    Player::Ctrl(Box::new(p))
                }
                _ => Player::Missing,
            },
        })
        .collect()
}

fn new_state(b: &Bundle, s: Rc<State>, depth: u32, next: &mut u64, at: f64) -> StateRun {
    let id = *next;
    *next += 1;
    StateRun { id, entries: entries(b, &s.animations, depth, next), def: s, entered: at }
}

fn tick_entries(w: &mut World, list: &mut [EntryRun], weight: f64, ctx: &mut Ctx) {
    for e in list {
        let ew = match &e.weight {
            None => 1.0,
            Some(s) => w.scope.run(s, ctx).num() as f64,
        };
        if ew != 0.0 && ew.is_finite() {
            tick_player(w, &mut e.player, weight * ew, ctx);
        }
    }
}

fn tick_player(w: &mut World, p: &mut Player, weight: f64, ctx: &mut Ctx) {
    match p {
        Player::Anim(a) => tick_anim(w, a, weight, ctx),
        Player::Ctrl(c) => tick_ctrl(w, c, weight, ctx),
        Player::Missing => {}
    }
}

fn finished(p: &Player) -> bool {
    match p {
        Player::Anim(a) => a.finished,
        Player::Ctrl(_) | Player::Missing => true,
    }
}

fn state_view(run: &StateRun, now: f64, clock: &mut Clock) {
    clock.state_time = now - run.entered;
    let anims: Vec<bool> =
        run.entries.iter().filter(|e| matches!(e.player, Player::Anim(_))).map(|e| finished(&e.player)).collect();
    clock.any_finished = anims.iter().any(|f| *f);
    clock.all_finished = anims.iter().all(|f| *f);
}

fn tick_ctrl(w: &mut World, c: &mut CtrlPlayer, weight: f64, ctx: &mut Ctx) {
    let Some(cur) = c.current.as_ref() else { return };
    let saved = *ctx.rt_clock;
    state_view(cur, w.seconds, ctx.rt_clock);
    ctx.rt_clock.anim_time = 0.0;
    // At most one transition a frame: the first whose condition holds.
    let mut target = None;
    for (to, cond) in &cur.def.transitions {
        if w.scope.run(cond, ctx).truthy() {
            target = Some(to.clone());
            break;
        }
    }
    if let Some(to) = target {
        if let Some(next) = c.def.states.get(&*to).cloned() {
            let cur = c.current.take().unwrap();
            for s in &cur.def.on_exit {
                w.scope.run(s, ctx);
            }
            w.effects.push(Effect::StopParticles { state: cur.id });
            let blend = cur.def.blend.clone();
            match c.previous.take() {
                Some(prev) if blend.seconds > 0.0 && w.seconds - c.fade_start < c.fade.seconds => {
                    c.older.push(Fade { run: prev, blend: c.fade.clone(), start: c.fade_start });
                }
                Some(prev) => {
                    w.effects.push(Effect::StopParticles { state: prev.id });
                    for f in c.older.drain(..) {
                        w.effects.push(Effect::StopParticles { state: f.run.id });
                    }
                }
                None => {}
            }
            c.previous = (blend.seconds > 0.0).then_some(cur);
            c.fade = blend;
            c.fade_start = w.seconds;
            let run = new_state(w.bundle, next.clone(), c.depth, w.next_state, w.seconds);
            state_view(&run, w.seconds, ctx.rt_clock);
            for s in &next.on_entry {
                w.scope.run(s, ctx);
            }
            for p in &next.particles {
                w.effects.push(Effect::Particle {
                    name: p.effect.clone(),
                    locator: p.locator.clone(),
                    state: Some(run.id),
                    bound: true,
                });
                if let Some(s) = &p.script {
                    w.scope.run(s, ctx);
                }
            }
            for s in &next.sounds {
                w.effects.push(Effect::Sound { name: s.clone(), locator: None });
            }
            c.current = Some(run);
        }
    }
    // Cross-fade: the state left fades out on its blend curve.
    let mut incoming = 1.0;
    if c.previous.is_some() {
        let t = w.seconds - c.fade_start;
        if t >= c.fade.seconds {
            let prev = c.previous.take().unwrap();
            w.effects.push(Effect::StopParticles { state: prev.id });
            for f in c.older.drain(..) {
                w.effects.push(Effect::StopParticles { state: f.run.id });
            }
        } else {
            let out = c.fade.outgoing(t).clamp(0.0, 1.0);
            incoming = 1.0 - out;
            // Older fades still running scale what the previous state gives.
            let mut chain = out;
            c.older.retain(|f| w.seconds - f.start < f.blend.seconds);
            for f in c.older.iter_mut().rev() {
                let o = f.blend.outgoing(w.seconds - f.start).clamp(0.0, 1.0);
                state_view(&f.run, w.seconds, ctx.rt_clock);
                tick_entries(w, &mut f.run.entries, weight * chain * o, ctx);
                chain *= 1.0 - o;
            }
            let prev = c.previous.as_mut().unwrap();
            state_view(prev, w.seconds, ctx.rt_clock);
            tick_entries(w, &mut prev.entries, weight * chain, ctx);
        }
    }
    let cur = c.current.as_mut().unwrap();
    state_view(cur, w.seconds, ctx.rt_clock);
    tick_entries(w, &mut cur.entries, weight * incoming, ctx);
    *ctx.rt_clock = Clock { delta: saved.delta, life: saved.life, ..saved };
}

fn tick_anim(w: &mut World, a: &mut AnimPlayer, weight: f64, ctx: &mut Ctx) {
    let dt = ctx.rt_clock.delta;
    if !a.started {
        a.started = true;
        if let Some(s) = &a.def.start_delay {
            ctx.rt_clock.anim_time = 0.0;
            a.start_delay = w.scope.run(s, ctx).num() as f64;
        }
    }
    if a.start_delay > 0.0 {
        a.start_delay -= dt;
        return;
    }
    let len = a.def.length.or(a.anim.as_ref().map(|x| x.length as f64)).unwrap_or(0.0);
    let looping = a.def.looping.or(a.anim.as_ref().map(|x| x.looping)).unwrap_or(Loop::Once);
    if a.loop_wait > 0.0 {
        a.loop_wait -= dt;
        if a.loop_wait > 0.0 {
            contribute(w, a, weight, ctx);
            return;
        }
        a.time = 0.0;
        a.prev = -1e-9;
    }
    ctx.rt_clock.anim_time = a.time;
    let mut t = match &a.def.anim_time_update {
        Some(s) => w.scope.run(s, ctx).num() as f64,
        None => a.time + dt,
    };
    if !t.is_finite() {
        t = a.time;
    }
    let mut contrib = true;
    if looping == Loop::Loop {
        if len <= 0.0 {
            a.finished = true;
        } else {
            let wrapped = ((t % len) + len) % len;
            if wrapped < a.prev - 1e-9 {
                fire(w, a, a.prev, len, ctx);
                a.finished = true;
                if let Some(d) = a.def.loop_delay.clone() {
                    ctx.rt_clock.anim_time = len;
                    let d = w.scope.run(&d, ctx).num() as f64;
                    if d > 0.0 {
                        a.loop_wait = d;
                        a.time = len;
                        a.prev = len;
                        contribute(w, a, weight, ctx);
                        return;
                    }
                }
                a.prev = -1e-9;
            }
            t = wrapped;
            fire(w, a, a.prev, t, ctx);
        }
    } else {
        let was = a.finished;
        if t >= len {
            t = len;
            a.finished = true;
        }
        if t < 0.0 {
            t = 0.0;
        }
        fire(w, a, a.prev, t, ctx);
        if looping == Loop::Once && was {
            contrib = false;
        }
    }
    a.time = t;
    a.prev = t;
    if contrib {
        contribute(w, a, weight, ctx);
    }
}

/// Timeline scripts, sounds and particles with `from < time <= to`: the
/// entity's own (on its animation entry) and the animation file's.
fn fire(w: &mut World, a: &AnimPlayer, from: f64, to: f64, ctx: &mut Ctx) {
    let hit = |t: f32| (t as f64) > from && (t as f64) <= to;
    let def = a.def.clone();
    let anim = a.anim.clone();
    let empty: Vec<crate::anim::Event> = Vec::new();
    let (at, asn, apt) = match &anim {
        Some(x) => (&x.timeline, &x.sounds, &x.particles),
        None => (&empty, &empty, &empty),
    };
    for e in def.timeline.iter().chain(at.iter()).filter(|e| hit(e.time)) {
        if let Some(s) = &e.script {
            w.scope.run(s, ctx);
        }
    }
    for e in def.sounds.iter().chain(asn.iter()).filter(|e| hit(e.time)) {
        if let Some(n) = &e.effect {
            w.effects.push(Effect::Sound { name: n.clone(), locator: e.locator.clone() });
        }
    }
    for e in def.particles.iter().chain(apt.iter()).filter(|e| hit(e.time)) {
        if let Some(s) = &e.script {
            w.scope.run(s, ctx);
        }
        if let Some(n) = &e.effect {
            w.effects.push(Effect::Particle { name: n.clone(), locator: e.locator.clone(), state: None, bound: false });
        }
    }
}

fn contribute(w: &mut World, a: &AnimPlayer, weight: f64, ctx: &mut Ctx) {
    ctx.rt_clock.anim_time = a.time;
    let bw = match &a.def.blend_weight {
        Some(s) => w.scope.run(s, ctx).num() as f64,
        None => 1.0,
    };
    let weight = weight * bw;
    if weight == 0.0 || !weight.is_finite() {
        return;
    }
    let Some(anim) = a.anim.clone() else { return };
    let wf = weight as f32;
    let keep = 1.0 - wf;
    let over = a.def.override_previous || anim.override_previous;
    let time = (a.time + 2.5e-5) as f32;
    for name in &a.def.relative_to_entity {
        w.relative.push(name.clone());
    }
    // An animation's channels share temps within one evaluation.
    w.scope.temps.clear();
    for b in &anim.bones {
        let cur = w.pose.get(&b.bone);
        let mut out = cur;
        for k in 0..3 {
            let Some(ch) = &b.channels[k] else { continue };
            let base = k * 3;
            let this = [cur[base], cur[base + 1], cur[base + 2]];
            let v = ch.sample(time, w.scope, ctx, this);
            for i in 0..3 {
                let j = base + i;
                out[j] = if k == 2 {
                    // Scale multiplies: 1 + w * (v - 1).
                    if over { cur[j] * keep + wf * v[i] } else { cur[j] * (1.0 + wf * (v[i] - 1.0)) }
                } else if over {
                    cur[j] * keep + wf * v[i]
                } else {
                    cur[j] + wf * v[i]
                };
            }
        }
        if crate::debug() && std::env::var("ANS_TRACE_BONE").as_deref().unwrap_or("rightarm") == &*b.bone && (0..3).any(|i| (out[i] - cur[i]).abs() > 1.0) {
            eprintln!("bone {} {} w {wf} rot {:?} -> {:?}", b.bone, anim.name, &cur[..3], &out[..3]);
        }
        *w.pose.bone(&b.bone) = out;
    }
}
