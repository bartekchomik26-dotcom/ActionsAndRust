// Composites: the mobs whose texture the jar composes, drawn, with how many
// quads landed on each part (and none left on the composed name).
// usage: composites <assets root>
// (was: every item with an attachable, held in each hand, first and third
// person; prints one JSON line per case with what its attachable drew
// (quads, sprites, bounds in the model frame). tools/sweep.py reads them.
// usage: sweep <assets root>
use ans_beyond_engine::assets::Dir;
use ans_beyond_engine::host::{Host, MobFrame};
use std::collections::BTreeMap;
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
    // The game's sizes (1.21 textures; the baby ones as the jar crops them).
    let sizes: &[(&str, [f32; 2])] = &[
        ("entity/sheep/sheep", [64.0, 32.0]),
        ("entity/sheep/sheep_wool", [64.0, 32.0]),
        ("entity/sheep/sheep_wool_undercoat", [64.0, 32.0]),
        ("entity/sheep/sheep_baby", [32.0, 32.0]),
        ("entity/sheep/sheep_wool_baby", [32.0, 32.0]),
    ];
    for (kind, age) in [("minecraft:sheep", 100.0), ("minecraft:sheep", -100.0)] {
        let mut host = Host::default();
        host.load_settings(&src);
        let want = host.composite_sprites();
        for (k, v) in sizes {
            host.sprite_sizes.insert(k.to_string(), *v);
        }
        host.change_color = 0xF9801D;
        let st = State { ground: true, ..Default::default() };
        let mut env = Mock(st);
        for i in 0..4 {
            host.begin_frame();
            let vars = [("is_first_person", 0.0)];
            let ctx = [("player_arm_height", 1.0)];
            let _ = age;
            let f = MobFrame { id: 7, kind, age_ticks: 100.0 + i as f64, hurt: false, on_fire: false, skin: None, vars: &vars, context: &ctx, held: ["", ""], worn: [""; 4] };
            host.draw_mob(&src, &f, &mut env);
        }
        let mut by: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for q in &host.quads {
            let e = by.entry(q.sprite.to_string()).or_default();
            e.0 += 1;
            if q.tint & 0xFFFFFF != 0xFFFFFF { e.1 += 1; }
            let bad = q.uv.iter().any(|u| *u < -0.01 || *u > 16.01);
            if bad { println!("  out of sprite: {} {:?}", q.sprite, q.uv); }
        }
        println!("{kind} composites wanted {} quads={} by sprite (count, tinted)={:?}", want.len(), host.quads.len(), by);
    }
}
