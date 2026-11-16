// Bench: what a draw costs per mob type (one core).\n// (was: every item with an attachable, held in each hand, first and third
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
    // A crowd: players (third person, held items), a first person view, and mobs.
    let kinds = ["minecraft:zombie", "minecraft:cow", "minecraft:sheep", "minecraft:skeleton", "minecraft:creeper", "minecraft:pig", "minecraft:chicken", "minecraft:villager"];
    let frames = 300;
    let mut per_kind: Vec<(String, f64)> = Vec::new();
    let only = std::env::var("ONLY").ok();
    let frames: usize = std::env::var("FRAMES").ok().and_then(|f| f.parse().ok()).unwrap_or(frames);
    let mut run = |label: &str, kind: &str, fp: bool, held: [&str; 2], n: u32, host: &mut Host| {
        if only.as_deref().is_some_and(|o| !label.contains(o)) {
            return;
        }
        let st = State { ground: true, speed: 0.2, walk: 0.0, item: held[0].into(), off: held[1].into(), ..Default::default() };
        let vars = [("is_first_person", if fp { 1.0 } else { 0.0 }), ("is_paperdoll", 0.0), ("map_face_icon", 0.0), ("bob_animation", 1.0), ("attack_time", 0.0)];
        let ctx = [("player_arm_height", 1.0), ("player_offhand_arm_height", 1.0)];
        let skin = if kind == "minecraft:player" { Some("entity/player/wide/steve") } else { None };
        // Warm up: data loads once per mob type.
        for i in 0..20 {
            host.begin_frame();
            for id in 0..n {
                let f = MobFrame { id: 1000 + id, kind, age_ticks: i as f64, hurt: false, on_fire: false, skin, vars: &vars, context: &ctx, held, worn: [""; 4] };
                let mut env = Mock(State { walk: i as f64 * 0.3, ..st.clone() });
                host.draw_mob(&src, &f, &mut env);
            }
        }
        let t = std::time::Instant::now();
        let quads = measured(host, &src, kind, skin, &vars, &ctx, held, n, frames, &st);
        let per = t.elapsed().as_secs_f64() * 1000.0 / (frames as f64 * n as f64);
        println!("{label:<34} {per:>8.4} ms per draw  ({} quads)", quads / (frames as usize * n as usize));
        per_kind.push((label.to_string(), per));
    };
    run("player 3rd person, sword", "minecraft:player", false, ["minecraft:netherite_sword", ""], 4, &mut host);
    run("player 3rd person, shield+bow", "minecraft:player", false, ["minecraft:bow", "minecraft:shield"], 4, &mut host);
    run("player 1st person, sword", "minecraft:player", true, ["minecraft:netherite_sword", ""], 1, &mut host);
    run("player 1st person, empty", "minecraft:player", true, ["", ""], 1, &mut host);
    for k in kinds {
        run(k, k, false, ["", ""], 8, &mut host);
    }
}

/// The measured frames (callgrind: `--toggle-collect=*measured*`).
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn measured(host: &mut Host, src: &Dir, kind: &str, skin: Option<&str>, vars: &[(&str, f32)], ctx: &[(&str, f32)], held: [&str; 2], n: u32, frames: usize, st: &State) -> usize {
    let mut quads = 0;
    for i in 0..frames {
        host.begin_frame();
        for id in 0..n {
            let f = MobFrame { id: 1000 + id, kind, age_ticks: 20.0 + i as f64 * 0.33, hurt: false, on_fire: false, skin, vars, context: ctx, held, worn: [""; 4] };
            let mut env = Mock(State { walk: i as f64 * 0.1, ..st.clone() });
            host.draw_mob(src, &f, &mut env);
            quads += host.quads.len();
        }
    }
    quads
}
