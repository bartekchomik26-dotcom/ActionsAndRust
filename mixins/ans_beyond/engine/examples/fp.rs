// First-person check: poses the player bundle with v.is_first_person and
// reports where the visible parts are (Horizon mob frame, blocks), then
// writes the quads for raster.py. usage: fp <assets root> <seconds> <swing>
use ans_beyond_engine::assets::Dir;
use ans_beyond_engine::host::{Host, MobFrame};
use ans_beyond_engine::molang::{Env, Scope, Value};

struct Mock;
impl Env for Mock {
    fn query(&mut self, _: &mut Scope, name: &str, args: &[Value]) -> Value {
        if name == "get_equipped_item_name" {
            let item = std::env::var("ITEM").unwrap_or_default();
            return Value::str(item.strip_prefix("minecraft:").unwrap_or(&item));
        }
        Value::Num(match name {
            "is_alive" | "is_on_ground" | "is_local_player" => 1.0,
            "distance_from_camera" => 0.0,
            "graphics_mode_is_any" => args.iter().any(|a| a.as_str() == Some("fancy")) as i32 as f32,
            "base_swing_duration" | "modified_swing_duration" => 0.3,
            "health" | "max_health" => 20.0,
            "model_scale" => 1.0,
            "is_item_name_any" => {
                let item = std::env::var("ITEM").unwrap_or_default();
                let main = args.first().and_then(|a| a.as_str()) == Some("slot.weapon.mainhand");
                let id = if main { item.as_str() } else { "" };
                args.iter().skip(1).filter_map(|a| a.as_str()).any(|n| n == id || (id.is_empty() && (n.is_empty() || n == "minecraft:air"))) as i32 as f32
            }
            "is_item_equipped" => (std::env::var("ITEM").is_ok() && args.first().map_or(0.0, |a| a.num()) < 1.0) as i32 as f32,
            "equipped_item_any_tag" => {
                let item = std::env::var("ITEM").unwrap_or_default();
                let main = args.first().and_then(|a| a.as_str()) == Some("slot.weapon.mainhand");
                (main && !item.is_empty() && args.iter().skip(1).filter_map(|a| a.as_str()).any(|t| {
                    let t = t.split(':').nth(1).unwrap_or(t);
                    t.strip_prefix("is_").is_some_and(|k| item.contains(k))
                })) as i32 as f32
            }
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
        let vars = [("is_first_person", 1.0), ("is_paperdoll", 0.0), ("map_face_icon", 0.0), ("bob_animation", 1.0),
            ("attack_time", if swing > 0.0 { (t * 3.0).fract() } else { 0.0 })];
        let ctx = [("player_arm_height", 1.0), ("player_offhand_arm_height", 1.0)];
        let f = MobFrame { id: 1, kind: "minecraft:player", age_ticks: 100.0 + i as f64 / 3.0, hurt: false, on_fire: false,
            skin: Some("entity/player/wide/steve"), vars: &vars, context: &ctx, held: [held.as_str(), ""], worn: [""; 4] };
        ok = host.draw_mob(&src, &f, &mut Mock);
    }
    assert!(ok);
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for q in &host.quads { for c in q.corners { for k in 0..3 { lo[k] = lo[k].min(c[k]); hi[k] = hi[k].max(c[k]); } } }
    eprintln!("quads {} bounds {:?} .. {:?}", host.quads.len(), lo, hi);
    let q: Vec<String> = host.quads.iter().map(|q| format!(
        "{{\"s\":\"{}\",\"c\":{:?},\"uv\":{:?},\"t\":{},\"fb\":{}}}", q.sprite, q.corners, q.uv, q.tint, q.full_bright)).collect();
    println!("{{\"quads\":[{}]}}", q.join(","));
}
