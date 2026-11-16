// First-person check: poses the player bundle with v.is_first_person and
// reports where the visible parts are (Horizon mob frame, blocks), then
// writes the quads for raster.py. usage: fp <assets root> <seconds> <swing>
use ans_beyond_engine::assets::Dir;
use ans_beyond_engine::host::{Host, MobFrame};
use ans_beyond_engine::molang::{Env, Scope, Value};

struct Mock;
impl Env for Mock {
    fn query(&mut self, _: &mut Scope, name: &str, args: &[Value]) -> Value {
        Value::Num(match name {
            "is_alive" | "is_on_ground" | "is_local_player" => 1.0,
            "graphics_mode_is_any" => args.iter().any(|a| a.as_str() == Some("fancy")) as i32 as f32,
            "base_swing_duration" | "modified_swing_duration" => 0.3,
            "health" | "max_health" => 20.0,
            "model_scale" => 1.0,
            "is_item_name_any" => args.iter().skip(1).any(|a| a.as_str() == Some("")) as i32 as f32,
            _ => 0.0,
        })
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = Dir(a[1].clone().into());
    let secs: f64 = a[2].parse().unwrap();
    let swing: f32 = a.get(3).map(|s| s.parse().unwrap()).unwrap_or(0.0);
    let mut host = Host::default();
    host.load_settings(&src);
    if let Ok(m) = std::env::var("MORPH") { host.settings.chosen.insert("oreville_ans:pntihm".into(), m); }
    let frames = (secs * 60.0) as usize;
    let mut ok = false;
    let held = std::env::var("ITEM").unwrap_or_default();
    for i in 0..frames {
        host.begin_frame();
        let t = i as f32 / frames as f32;
        let vars = [("is_first_person", 0.0), ("is_paperdoll", 0.0), ("map_face_icon", 0.0), ("bob_animation", 1.0),
            ("attack_time", if swing > 0.0 { (t * 3.0).fract() } else { 0.0 })];
        let ctx = [("player_arm_height", 1.0), ("player_offhand_arm_height", 1.0)];
        let f = MobFrame { id: 1, kind: "minecraft:player", age_ticks: 100.0 + i as f64 / 3.0, hurt: false, on_fire: false,
            skin: Some("entity/player/wide/steve"), vars: &vars, context: &ctx, held: [held.as_str(), ""], worn: [""; 4] };
        ok = host.draw_mob(&src, &f, &mut Mock);
        if std::env::var_os("ARMOR").is_some() {
            let p = |slot| ans_beyond_engine::render::ArmorPiece { slot, texture: "minecraft:textures/entity/equipment/humanoid/iron.png".into(), tint: 0xFF6060 };
            host.armor(&[p(0), p(1), p(3)]);
        }
    }
    assert!(ok);
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for q in &host.quads { for c in q.corners { for k in 0..3 { lo[k] = lo[k].min(c[k]); hi[k] = hi[k].max(c[k]); } } }
    eprintln!("quads {} bounds {:?} .. {:?}", host.quads.len(), lo, hi);
    for (i, h) in host.hands.iter().enumerate() { if let Some(m) = h { eprintln!("hand {i} at {:?}", [m.0[0][3], m.0[1][3], m.0[2][3]]); } else { eprintln!("hand {i} none"); } }
    let q: Vec<String> = host.quads.iter().map(|q| format!(
        "{{\"s\":\"{}\",\"c\":{:?},\"uv\":{:?},\"t\":{},\"fb\":{}}}", q.sprite, q.corners, q.uv, q.tint, q.full_bright)).collect();
    println!("{{\"quads\":[{}]}}", q.join(","));
}
