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
    let mut host = Host::default();
    host.load_settings(&src);
    let mode = std::env::var("MODE").unwrap_or_default();
    let st = State { ground: !mode.contains("fly"), item: std::env::var("ITEM").unwrap_or("minecraft:lead".into()), swim: mode.contains("swim"), water: mode.contains("swim"), sprint: mode.contains("sprint"), speed: if mode.is_empty() { 0.0 } else { 0.28 }, vy: 0.0, ..Default::default() };
    for i in 0..40 {
        host.begin_frame();
        let vars = [("is_first_person", 1.0), ("is_paperdoll", 0.0), ("map_face_icon", 0.0), ("bob_animation", 1.0), ("attack_time", 0.0)];
        let ctx = [("player_arm_height", 1.0), ("player_offhand_arm_height", 1.0)];
        let f = MobFrame { id: 1, kind: "minecraft:player", age_ticks: 100.0 + i as f64, hurt: false, on_fire: false, skin: Some("entity/player/wide/steve"), vars: &vars, context: &ctx, held: [st.item.as_str(), ""], worn: [""; 4] };
        let mut env = Mock(st.clone());
        host.draw_mob(&src, &f, &mut env);
    }
    println!("held_model={:?}", host.held_model);
    let eye = [0.12f32, 1.42, 0.0];
    for &(a, b) in &host.cube_spans {
        let mut lo = [f32::MAX; 3]; let mut hi = [f32::MIN; 3];
        for q in &host.quads[a..b] { for c in &q.corners { for i in 0..3 { lo[i] = lo[i].min(c[i]); hi[i] = hi[i].max(c[i]); } } }
        let d: f32 = (0..3).map(|i| (lo[i] - eye[i]).max(0.0).max(eye[i] - hi[i]).powi(2)).sum::<f32>().sqrt();
        println!("cube {a}..{b} {} dist={d:.3} lo={lo:?} hi={hi:?}", host.quads[a].sprite);
    }
    for (i, q) in host.quads.iter().enumerate().take(0) {
        println!("{i} {} uv={:?} corners={:?}", q.sprite, q.uv.map(|u| u * 4.0), q.corners);
    }
}
