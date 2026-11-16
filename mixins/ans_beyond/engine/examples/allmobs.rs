// Poses an entity for a few seconds of game time with a scripted mock of
// the game's queries and writes its quads as JSON, for previews and tests.
// usage: preview <assets root> <entity> <seconds> <scenario>
use ans_beyond_engine::assets::{Dir, Library};
use ans_beyond_engine::molang::{Env, Scope, Value};
use ans_beyond_engine::render::{self, RenderState};
use ans_beyond_engine::runtime::Runtime;

struct Mock { t: f64, scenario: String }
impl Env for Mock {
    fn query(&mut self, _: &mut Scope, name: &str, args: &[Value]) -> Value {
        let walking = self.scenario.contains("walk");
        let dead = self.scenario.contains("dead");
        Value::Num(match name {
            "is_alive" => if dead { 0.0 } else { 1.0 },
            "death_ticks" => if dead { (self.t * 20.0) as f32 } else { 0.0 },
            "is_on_ground" => 1.0,
            "modified_move_speed" | "ground_speed" => if walking { 0.5 } else { 0.0 },
            "modified_distance_moved" | "walk_distance" => if walking { (self.t * 4.0) as f32 } else { 0.0 },
            "is_moving" => walking as i32 as f32,
            "health" | "max_health" => 20.0,
            "graphics_mode_is_any" => args.iter().any(|a| a.as_str() == Some("fancy")) as i32 as f32,
            "is_pack_setting_enabled" | "is_pack_setting_selected" => 0.0,
            "model_scale" => 1.0,
            "base_swing_duration" | "modified_swing_duration" => 0.3,
            "frame_alpha" => 0.0,
            "time_stamp" => (self.t * 20.0) as f32,
            "is_owner_identifier_any" => 0.0,
            _ => 0.0,
        })
    }
}


fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = Dir(a[1].clone().into());
    let mut names: Vec<String> = std::fs::read_dir(format!("{}/assets/ans/entity", a[1])).unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".json") && !n.starts_with('_')).map(|n| n.trim_end_matches(".json").to_string()).collect();
    names.sort();
    for n in names {
        let mut lib = Library::default();
        let t0 = std::time::Instant::now();
        let Some(bundle) = lib.bundle(&src, &n) else { println!("{n}\tNO_BUNDLE"); continue };
        let load = t0.elapsed().as_secs_f64() * 1000.0;
        let mut rt = Runtime::new(bundle.clone(), 7);
        let mut env = Mock { t: 0.0, scenario: "walk".into() };
        let dt = 1.0 / 60.0;
        let t1 = std::time::Instant::now();
        for i in 0..180 { env.t = i as f64 * dt; rt.tick(dt, env.t + 5.0, &mut env); }
        let tick = t1.elapsed().as_secs_f64() * 1000.0 / 180.0;
        let st = RenderState { hurt: false, on_fire: false };
        let ls = render::layers(&bundle, &mut rt.scope, &mut env, &st, &mut |id| lib.geometry(&src, id));
        let mut quads = Vec::new();
        for l in &ls { if let Some(g) = lib.geometry(&src, &l.geometry) { let m = render::bone_matrices(&g, &rt.pose, &rt.entity_relative); render::mesh(&g, l, &m, [1.0;3], 0.0, &mut quads); } }
        let anims = bundle.targets.len();
        let missing = bundle.targets.values().filter(|t| matches!(t, ans_beyond_engine::bundle::Target::Anim(a) if bundle.animation(&a.animation).is_none())).count();
        println!("{n}\tload {load:.0}ms\ttick {tick:.3}ms\tlayers {}\tquads {}\tbones {}\ttargets {anims}\tmissing_anims {missing}", ls.len(), quads.len(), rt.pose.bones.len());
    }
}
