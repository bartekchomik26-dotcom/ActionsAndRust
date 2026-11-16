// Sweep: every item with an attachable, held in each hand, first and third
// person; prints one JSON line per case with what its attachable drew
// (quads, sprites, bounds in the model frame). tools/sweep.py reads them.
// usage: sweep <assets root>
use ans_beyond_engine::assets::Dir;
use ans_beyond_engine::host::{Host, MobFrame};
use ans_beyond_engine::molang::{Env, Scope, Value};

#[derive(Default, Clone)]
struct State {
    pos: [f64; 3],
    speed: f64,
    vy: f64,
    ground: bool,
    sneak: bool,
    sprint: bool,
    swim: bool,
    water: bool,
    using: f64,
    item: String,
    off: String,
    walk: f64,
}

struct Mock(State);
impl Env for Mock {
    fn query(&mut self, _: &mut Scope, name: &str, args: &[Value]) -> Value {
        let s = &self.0;
        let item = s.item.as_str();
        let main_slot = || args.first().and_then(|a| a.as_str()).is_none_or(|a| a.contains("mainhand"));
        match name {
            "get_equipped_item_name" => {
                let it = if main_slot() { item } else { s.off.as_str() };
                return Value::str(it.strip_prefix("minecraft:").unwrap_or(it));
            }
            "ans_tick_position" | "position" => return Value::Num(s.pos[(args.first().map_or(0.0, |a| a.num()) as usize).min(2)] as f32),
            _ => {}
        }
        let b = |v: bool| v as i32 as f32;
        Value::Num(match name {
            "is_alive" | "is_local_player" => 1.0,
            "is_on_ground" => b(s.ground),
            "is_sneaking" => b(s.sneak),
            "is_sprinting" => b(s.sprint),
            "is_swimming" | "is_crawling" => b(s.swim),
            "is_in_water" | "is_in_water_or_rain" => b(s.water),
            "is_jumping" => b(!s.ground && s.vy > 0.0),
            "is_moving" => b(s.speed > 0.0),
            // Walking straight ahead (+z, the way the body faces).
            "movement_direction" => match args.first().map_or(0.0, |a| a.num()) as i32 {
                2 if s.speed > 0.0 => 1.0,
                _ => 0.0,
            },
            "ground_speed" => (s.speed * 20.0) as f32,
            "modified_move_speed" => (s.speed * 20.0 / 4.3).min(1.0) as f32,
            "vertical_speed" => (s.vy * 20.0) as f32,
            "modified_distance_moved" | "walk_distance" => s.walk as f32,
            "is_using_item" => b(s.using > 0.0),
            "main_hand_item_use_duration" => (s.using / 20.0) as f32,
            "main_hand_item_max_duration" => 72000.0 / 20.0,
            "blocking" => b(s.using > 0.0 && item.contains("shield")),
            "distance_from_camera" => 4.0,
            "graphics_mode_is_any" => b(args.iter().any(|a| a.as_str() == Some("fancy"))),
            "base_swing_duration" | "modified_swing_duration" => 0.3,
            "health" | "max_health" => 20.0,
            "model_scale" => 1.0,
            "is_item_name_any" => {
                let id = if main_slot() { item } else { s.off.as_str() };
                b(args.iter().skip(1).filter_map(|a| a.as_str()).any(|n| n == id || (id.is_empty() && (n.is_empty() || n == "minecraft:air"))))
            }
            "is_item_equipped" => b(!item.is_empty() && args.first().map_or(0.0, |a| a.num()) < 1.0),
            "equipped_item_any_tag" => b(main_slot()
                && !item.is_empty()
                && args.iter().skip(1).filter_map(|a| a.as_str()).any(|t| {
                    let t = t.split(':').nth(1).unwrap_or(t);
                    t.strip_prefix("is_").is_some_and(|k| item.contains(k))
                        || (t == "is_food" && ["beef", "bread", "apple", "porkchop"].iter().any(|f| item.contains(f)))
                        || (t == "is_tool" && ["pickaxe", "axe", "shovel", "hoe", "sword"].iter().any(|f| item.contains(f)))
                })),
            _ => 0.0,
        })
    }
}


fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = Dir(a[1].clone().into());
    let index = std::fs::read_to_string(format!("{}/assets/ans/attachable/_index.json", a[1])).unwrap();
    let j = ans_beyond_engine::json::parse(&index).unwrap();
    let mut items: Vec<String> = j.get("items").unwrap().entries().iter().map(|(k, _)| k.clone()).collect();
    items.sort();
    if let Ok(only) = std::env::var("ONLY") {
        items.retain(|i| if let Some(x) = only.strip_suffix('$') { i == x } else { i.contains(&only) });
    }
    let mut host = Host::default();
    host.load_settings(&src);
    // SET=id=value,...: pack settings as a player may have them.
    if let Ok(set) = std::env::var("SET") {
        for kv in set.split(',') {
            if let Some((k, v)) = kv.split_once('=') {
                host.settings.chosen.insert(k.into(), v.into());
            }
        }
    }
    let mut next_id = 1u32;
    for item in &items {
        for slot in 0..2usize {
            for fp in [true, false] {
                next_id += 1;
                let id = next_id;
                let held = if slot == 0 { [item.as_str(), ""] } else { ["", item.as_str()] };
                let st = State { ground: true, item: held[0].into(), off: held[1].into(), ..Default::default() };
                let mut last = None;
                let frames = if std::env::var_os("SWING").is_some() && fp { 30 } else { 6 };
                let mut near = 0usize;
                let eye: Vec<f32> = std::env::var("EYE").unwrap_or("0.12,1.42,0".into()).split(',').map(|x| x.parse().unwrap()).collect();
                let mut near_min = f32::MAX;
                for i in 0..frames {
                    host.begin_frame();
                    let vars = [("is_first_person", if fp { 1.0 } else { 0.0 }), ("is_paperdoll", 0.0), ("map_face_icon", 0.0), ("bob_animation", 1.0), ("attack_time", if frames > 6 && i >= 6 { (i - 6) as f32 / 24.0 } else { 0.0 }), ("is_holding_right", if slot == 0 { 1.0 } else { 0.0 }), ("is_holding_left", if slot == 1 { 1.0 } else { 0.0 })];
                    let ctx = [("player_arm_height", 1.0), ("player_offhand_arm_height", 1.0)];
                    let f = MobFrame { id, kind: "minecraft:player", age_ticks: 100.0 + i as f64 / 3.0, hurt: false, on_fire: false, skin: Some("entity/player/wide/steve"), vars: &vars, context: &ctx, held, worn: [""; 4] };
                    let mut env = Mock(st.clone());
                    host.draw_mob(&src, &f, &mut env);
                    last = Some(());
                    if fp {
                        let (a, b) = host.att_quads[slot];
                        for q in &host.quads[a.min(host.quads.len())..b.min(host.quads.len())] {
                            // Nearest point of the quad's corners to the eye.
                            for c in &q.corners {
                                let d = ((c[0] - eye[0]) * (c[0] - eye[0]) + (c[1] - eye[1]) * (c[1] - eye[1]) + (c[2] - eye[2]) * (c[2] - eye[2])).sqrt();
                                near_min = near_min.min(d);
                            }
                            let c = [0, 1, 2].map(|k| q.corners.iter().map(|p| p[k]).sum::<f32>() / 4.0);
                            let d = ((c[0] - eye[0]) * (c[0] - eye[0]) + (c[1] - eye[1]) * (c[1] - eye[1]) + (c[2] - eye[2]) * (c[2] - eye[2])).sqrt();
                            if d < 0.25 { near += 1; }
                        }
                    }
                }
                let _ = last;
                let (s, e) = host.att_quads[slot];
                let qs = &host.quads[s.min(host.quads.len())..e.min(host.quads.len())];
                let mut lo = [f32::MAX; 3];
                let mut hi = [f32::MIN; 3];
                let mut nan = false;
                let mut sprites: Vec<String> = Vec::new();
                let mut uvs: Vec<String> = Vec::new();
                for q in qs {
                    for c in &q.corners {
                        for k in 0..3 {
                            if !c[k].is_finite() { nan = true; continue; }
                            lo[k] = lo[k].min(c[k]);
                            hi[k] = hi[k].max(c[k]);
                        }
                    }
                    if !sprites.iter().any(|x| **x == *q.sprite) { sprites.push(q.sprite.to_string()); }
                    if uvs.len() < 400 { uvs.push(format!("[\"{}\",{},{},{},{},{}]", q.sprite, q.uv[0], q.uv[1], q.uv[2], q.uv[3], q.full_bright)); }
                }
                let attached = host.drew_attachable(id, slot);
                println!("{{\"item\":\"{item}\",\"slot\":{slot},\"fp\":{fp},\"attached\":{attached},\"quads\":{},\"nan\":{nan},\"near\":{near},\"near_min\":{near_min},\"lo\":{:?},\"hi\":{:?},\"sprites\":{:?},\"uvs\":[{}]}}", qs.len(), lo, hi, sprites, uvs.join(","));
            }
        }
    }
}
