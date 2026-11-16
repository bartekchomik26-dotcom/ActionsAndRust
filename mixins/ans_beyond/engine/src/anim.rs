// Bedrock animations: per-bone rotation, position and scale channels, each
// a constant or keyframes (pre/post values, linear, step or Catmull-Rom),
// every axis a Molang script; plus sound, particle and timeline events.

use crate::json::Json;
use crate::molang::{self, Env, Scope, Script};


#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lerp {
    Linear,
    CatmullRom,
    Step,
}

#[derive(Clone, Debug)]
pub struct Key {
    pub time: f32,
    pub pre: [Script; 3],
    pub post: [Script; 3],
    pub lerp: Lerp,
}

#[derive(Clone, Debug)]
pub enum Channel {
    Const([Script; 3]),
    Keys(Vec<Key>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rotation = 0,
    Position = 1,
    Scale = 2,
}

#[derive(Clone, Debug)]
pub struct BoneAnim {
    pub bone: String,
    pub channels: [Option<Channel>; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Loop {
    Once,
    Loop,
    HoldOnLastFrame,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub time: f32,
    /// A sound or particle effect name (the entity's short name), or a script.
    pub effect: Option<String>,
    pub locator: Option<String>,
    pub script: Option<Script>,
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub name: String,
    pub looping: Loop,
    pub length: f32,
    pub anim_time_update: Option<Script>,
    pub blend_weight: Option<Script>,
    pub start_delay: Option<Script>,
    pub loop_delay: Option<Script>,
    pub override_previous: bool,
    pub bones: Vec<BoneAnim>,
    pub sounds: Vec<Event>,
    pub particles: Vec<Event>,
    pub timeline: Vec<Event>,
}

pub fn script(j: &Json) -> Script {
    match j {
        Json::Num(n) => Script::constant(*n as f32),
        Json::Bool(b) => Script::constant(*b as i32 as f32),
        Json::Str(s) => molang::compile_or_zero(s),
        _ => Script::constant(0.0),
    }
}

fn vec3(j: &Json) -> [Script; 3] {
    match j {
        Json::Arr(a) if a.len() >= 3 => [script(&a[0]), script(&a[1]), script(&a[2])],
        Json::Arr(a) if !a.is_empty() => [script(&a[0]), script(&a[0]), script(&a[0])],
        other => [script(other), script(other), script(other)],
    }
}

fn channel(j: &Json) -> Channel {
    match j {
        Json::Obj(kv) if kv.iter().all(|(k, _)| k.trim().parse::<f32>().is_ok()) && !kv.is_empty() => {
            let mut keys: Vec<Key> = kv
                .iter()
                .map(|(t, v)| {
                    let time = t.trim().parse().unwrap_or(0.0);
                    match v {
                        Json::Obj(_) => {
                            let post = v.get("post").or_else(|| v.get("pre")).map(vec3);
                            let pre = v.get("pre").map(vec3).or_else(|| post.clone());
                            let lerp = match v.get("lerp_mode").and_then(Json::as_str) {
                                Some("catmullrom") => Lerp::CatmullRom,
                                Some("step") => Lerp::Step,
                                _ => Lerp::Linear,
                            };
                            let z = || [Script::constant(0.0), Script::constant(0.0), Script::constant(0.0)];
                            Key { time, pre: pre.unwrap_or_else(z), post: post.unwrap_or_else(z), lerp }
                        }
                        other => {
                            let v = vec3(other);
                            Key { time, pre: v.clone(), post: v, lerp: Lerp::Linear }
                        }
                    }
                })
                .collect();
            keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            Channel::Keys(keys)
        }
        other => Channel::Const(vec3(other)),
    }
}

pub fn events(j: Option<&Json>, effect_key: &str) -> Vec<Event> {
    let mut out = Vec::new();
    for (t, v) in j.map(Json::entries).unwrap_or(&[]) {
        let time = t.trim().parse().unwrap_or(0.0);
        let items: Vec<&Json> = match v {
            Json::Arr(a) => a.iter().collect(),
            other => vec![other],
        };
        for it in items {
            match it {
                Json::Str(s) => out.push(Event { time, effect: None, locator: None, script: Some(molang::compile_or_zero(s)) }),
                Json::Obj(_) => out.push(Event {
                    time,
                    effect: it.get(effect_key).and_then(Json::as_str).map(str::to_string),
                    locator: it.get("locator").and_then(Json::as_str).map(str::to_ascii_lowercase),
                    script: it.get("pre_effect_script").and_then(Json::as_str).map(molang::compile_or_zero),
                }),
                _ => {}
            }
        }
    }
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    out
}

pub fn parse_file(j: &Json) -> crate::FastMap<String, Animation> {
    let mut out = crate::FastMap::default();
    for (name, a) in j.get("animations").map(Json::entries).unwrap_or(&[]) {
        out.insert(name.clone(), parse_animation(name, a));
    }
    out
}

pub fn parse_animation(name: &str, a: &Json) -> Animation {
    let looping = match a.get("loop") {
        Some(Json::Bool(true)) => Loop::Loop,
        Some(Json::Str(s)) if s == "hold_on_last_frame" => Loop::HoldOnLastFrame,
        Some(Json::Str(s)) if s == "true" => Loop::Loop,
        _ => Loop::Once,
    };
    let mut bones = Vec::new();
    let mut last_key: f32 = 0.0;
    for (bone, b) in a.get("bones").map(Json::entries).unwrap_or(&[]) {
        let mut channels = [None, None, None];
        for (i, k) in ["rotation", "position", "scale"].iter().enumerate() {
            if let Some(c) = b.get(k) {
                let c = channel(c);
                if let Channel::Keys(keys) = &c {
                    if let Some(l) = keys.last() {
                        last_key = last_key.max(l.time);
                    }
                }
                channels[i] = Some(c);
            }
        }
        bones.push(BoneAnim { bone: bone.to_ascii_lowercase(), channels });
    }
    let sounds = events(a.get("sound_effects"), "effect");
    let particles = events(a.get("particle_effects"), "effect");
    let timeline = events(a.get("timeline"), "");
    for e in sounds.iter().chain(&particles).chain(&timeline) {
        last_key = last_key.max(e.time);
    }
    let opt = |k: &str| a.get(k).map(script);
    Animation {
        name: name.to_string(),
        looping,
        length: a.get("animation_length").and_then(Json::as_f64).map(|x| x as f32).unwrap_or(last_key),
        anim_time_update: opt("anim_time_update"),
        blend_weight: opt("blend_weight"),
        start_delay: opt("start_delay"),
        loop_delay: opt("loop_delay"),
        override_previous: a.get("override_previous_animation").and_then(Json::as_bool).unwrap_or(false),
        bones,
        sounds,
        particles,
        timeline,
    }
}

fn eval3(s: &[Script; 3], scope: &mut Scope, env: &mut dyn Env, this: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0; 3];
    for i in 0..3 {
        out[i] = match s[i].as_const() {
            Some(c) => c,
            None => {
                scope.this = this[i];
                scope.run_keep_temps(&s[i], env).num()
            }
        };
    }
    out
}

fn catmull(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1) + (-p0 + p2) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

impl Channel {
    /// The channel's value at `time` seconds. `this` is the value the bone
    /// has so far, per axis, for scripts that read `this`.
    pub fn sample(&self, time: f32, scope: &mut Scope, env: &mut dyn Env, this: [f32; 3]) -> [f32; 3] {
        let keys = match self {
            Channel::Const(v) => return eval3(v, scope, env, this),
            Channel::Keys(k) => k,
        };
        let Some(first) = keys.first() else { return [0.0; 3] };
        if keys.len() == 1 || time <= first.time {
            return eval3(if time < first.time { &first.pre } else { &first.post }, scope, env, this);
        }
        let last = keys.last().unwrap();
        if time >= last.time {
            return eval3(&last.post, scope, env, this);
        }
        let i = keys.partition_point(|k| k.time <= time) - 1;
        let (a, b) = (&keys[i], &keys[i + 1]);
        let span = b.time - a.time;
        let t = if span > 0.0 { (time - a.time) / span } else { 1.0 };
        if a.lerp == Lerp::Step {
            return eval3(&a.post, scope, env, this);
        }
        let p1 = eval3(&a.post, scope, env, this);
        let p2 = eval3(&b.pre, scope, env, this);
        if a.lerp == Lerp::CatmullRom || b.lerp == Lerp::CatmullRom {
            let p0 = if i > 0 { eval3(&keys[i - 1].post, scope, env, this) } else { p1 };
            let p3 = if i + 2 < keys.len() { eval3(&keys[i + 2].pre, scope, env, this) } else { p2 };
            return [0, 1, 2].map(|k| catmull(p0[k], p1[k], p2[k], p3[k], t));
        }
        [0, 1, 2].map(|k| p1[k] + (p2[k] - p1[k]) * t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molang::NoEnv;

    fn anim(src: &str) -> Animation {
        let j = crate::json::parse(src).unwrap();
        parse_file(&j).into_values().next().unwrap()
    }

    #[test]
    fn keyframes_linear_step_and_length() {
        let a = anim(r#"{"animations":{"a":{"loop":"hold_on_last_frame","bones":{"Head":{
            "rotation":{"0.0":[0,0,0],"1.0":{"pre":[10,0,0],"post":[20,0,0]},"2.0":[20,0,0]},
            "position":[0,"q.life_time*2",0]}}}}}"#);
        assert_eq!(a.looping, Loop::HoldOnLastFrame);
        assert_eq!(a.length, 2.0);
        assert_eq!(a.bones[0].bone, "head");
        let mut s = Scope::new(1);
        let r = a.bones[0].channels[0].as_ref().unwrap();
        assert_eq!(r.sample(0.5, &mut s, &mut NoEnv, [0.0; 3])[0], 5.0);
        // `post` takes over at the key
        assert_eq!(r.sample(1.0, &mut s, &mut NoEnv, [0.0; 3])[0], 20.0);
        assert_eq!(r.sample(9.0, &mut s, &mut NoEnv, [0.0; 3])[0], 20.0);
    }

    #[test]
    fn catmullrom_passes_through_keys() {
        let a = anim(r#"{"animations":{"a":{"bones":{"b":{"position":{
            "0":{"post":[0,0,0],"lerp_mode":"catmullrom"},"1":{"post":[1,0,0],"lerp_mode":"catmullrom"},
            "2":{"post":[0,0,0],"lerp_mode":"catmullrom"}}}}}}}"#);
        let c = a.bones[0].channels[1].as_ref().unwrap();
        let mut s = Scope::new(1);
        assert!((c.sample(1.0, &mut s, &mut NoEnv, [0.0; 3])[0] - 1.0).abs() < 1e-6);
        let mid = c.sample(0.5, &mut s, &mut NoEnv, [0.0; 3])[0];
        assert!(mid > 0.5 && mid < 0.7, "{mid}");
    }
}
