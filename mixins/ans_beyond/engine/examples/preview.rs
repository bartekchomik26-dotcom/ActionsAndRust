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
    let mut lib = Library::default();
    let bundle = lib.bundle(&src, &a[2]).expect("no bundle");
    let seconds: f64 = a[3].parse().unwrap();
    let mut rt = Runtime::new(bundle.clone(), 7);
    let mut env = Mock { t: 0.0, scenario: a.get(4).cloned().unwrap_or_default() };
    let dt = 1.0 / 60.0;
    let start = std::time::Instant::now();
    let frames = (seconds / dt) as usize;
    for i in 0..frames {
        env.t = i as f64 * dt;
        rt.tick(dt, env.t + 5.0, &mut env);
    }
    let per_frame = start.elapsed().as_secs_f64() / frames.max(1) as f64;
    let st = RenderState { hurt: false, on_fire: false };
    let ls = render::layers(&bundle, &mut rt.scope, &mut env, &st, &mut |id| lib.geometry(&src, id));
    let mut quads = Vec::new();
    let mut layers_out = Vec::new();
    for l in &ls {
        let g = lib.geometry(&src, &l.geometry).unwrap();
        let m = render::bone_matrices(&g, &rt.pose, &rt.entity_relative);
        render::mesh(&g, l, &m, [1.0; 3], 0.0, &mut quads);
        layers_out.push(format!("{{\"geometry\":\"{}\",\"texture\":\"{}\"}}", l.geometry, l.texture));
    }
    let q: Vec<String> = quads.iter().map(|q| format!(
        "{{\"s\":\"{}\",\"c\":{:?},\"uv\":{:?},\"t\":{},\"fb\":{}}}", q.sprite, q.corners, q.uv, q.tint, q.full_bright)).collect();
    let bones: Vec<String> = rt.pose.bones.iter().map(|(k, v)| format!("\"{k}\":{v:?}")).collect();
    println!("{{\"per_frame_ms\":{},\"layers\":[{}],\"bones\":{{{}}},\"effects\":{},\"quads\":[{}]}}",
        per_frame * 1000.0, layers_out.join(","), bones.join(","), rt.effects.len(), q.join(","));
}
