// Actions & Beyond for Horizon: the game side. Becomes
// `crate::game::ans_beyond`. It reads Horizon's mob state for the Bedrock
// queries, feeds the engine (crate `ans_beyond_engine`), draws its quads
// through `native_draw_quad`, and plays the sounds the animations fire.
//
// Built against Horizon nightly 2026-11-08 (fdc87ae): it reads `Game`'s
// mob list directly, which no hook point exposes, so it is pinned there.

use ans_beyond_engine::host::{Host, MobFrame};
use ans_beyond_engine::molang::{Env, Expr, Scope, Value};
use ans_beyond_engine::assets::Source;
use horizon_client::mobs::Mob;
use std::cell::RefCell;
use ans_beyond_engine::render;
use horizon_client::items::ItemStack;

pub const ID: &str = "ans_beyond";
pub const NAME: &str = "Actions & Beyond";
pub const VERSION: &str = "1.0.0";
pub const DESCRIPTION: &str =
    "Bedrock mob models and animations from Oreville Studios' Actions & Stuff, ported from the Actions & Beyond Fabric mod.";

/// Light for a part that glows: full sky and block light.
const FULL_BRIGHT: u32 = 240 | 240 << 8;

/// The local player's id for the engine: it isn't in the mob list.
const LOCAL: u32 = u32::MAX;

thread_local! {
    static HOST: RefCell<Host> = RefCell::new(Host::default());
    /// The local player's Bedrock walk cycle (A&B's WalkAnimationMixin:
    /// `update(min(distance * 4, 1), 0.4, 1)` each tick): position, speed, tick.
    static WALK: RefCell<(f32, f32, f64)> = const { RefCell::new((0.0, 0.0, 0.0)) };
    static SETTINGS_READ: RefCell<bool> = const { RefCell::new(false) };
}

/// The game's resources (vanilla, packs, then content mods) as the engine's
/// source: the Bedrock data ships in the `ans_beyond` content mod.
struct Resources(horizon_client::client_resources::ClientResources);
impl Source for Resources {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.0.read(path)
    }
}

/// The Bedrock queries for one mob, answered from Horizon's state.
struct MobEnv<'a, 'g> {
    game: &'a super::Game<'g>,
    mob: Option<&'a Mob>,
    render: &'a super::NativeMobRender<'a>,
    who: Who<'a>,
}

fn has_name(args: &[Value], name: &str) -> bool {
    args.iter().any(|a| a.as_str().is_some_and(|s| s.eq_ignore_ascii_case(name)))
}

/// `slot.armor.head` and friends as `Mob::equipment_stacks` indices.
fn slot_index(slot: &str) -> Option<usize> {
    Some(match slot {
        "slot.weapon.mainhand" => 0,
        "slot.weapon.offhand" => 1,
        "slot.armor.feet" => 2,
        "slot.armor.legs" => 3,
        "slot.armor.chest" => 4,
        "slot.armor.head" => 5,
        "slot.armor.body" | "slot.armor" => 6,
        _ => return None,
    })
}

impl Env for MobEnv<'_, '_> {
    fn query(&mut self, _scope: &mut Scope, name: &str, args: &[Value]) -> Value {
        let r = self.render;
        let m = self.mob;
        let num = |x: f64| Value::Num(x as f32);
        let arg_num = |i: usize| args.get(i).map(Value::num).unwrap_or(0.0);
        if let Some(v) = shared_query(self.game, &self.who, name, args) {
            return v;
        }
        // This window's player in third person: Horizon's model of it carries
        // no ground, sprint or jump flags (it is synced for vanilla's model
        // only), so these come from the player itself.
        if self.who.hotbar.is_some()
            && let Some(v) = local_armor_query(self.game, name, args)
        {
            return v;
        }
        if self.who.hotbar.is_some()
            && let Some(p) = self.game.player.as_ref()
        {
            match name {
                "is_on_ground" => return Value::bool(p.ground.on_ground),
                "is_sprinting" => return Value::bool(p.sprinting),
                "is_jumping" => return Value::bool(p.jumping),
                "is_gliding" => return Value::bool(p.fall_flying),
                "is_on_fire" => return Value::bool(p.fire_ticks > 0),
                "health" => return num(f64::from(p.health.health())),
                "max_health" => return Value::Num(20.0),
                _ => {}
            }
        }
        match name {
            "is_alive" => Value::bool(r.death_time == 0 && m.is_none_or(|m| m.health > 0.0)),
            "death_ticks" => num(f64::from(r.death_time)),
            "is_baby" => Value::bool(r.baby),
            "is_on_ground" => Value::bool(r.on_ground),
            "is_in_ui" | "is_attached" | "is_emoting" | "is_spectator" => Value::ZERO,
            "graphics_mode_is_any" => Value::bool(has_name(args, "fancy") || has_name(args, "deferred")),
            "is_name_any" => Value::bool(r.custom_name.is_some_and(|n| has_name(args, n))),
            "get_name" => Value::str(r.custom_name.unwrap_or("")),
            "is_owner_identifier_any" => Value::bool(has_name(args, r.kind)),
            "modified_move_speed" | "walk_speed" => num(f64::from(r.walk_speed)),
            "modified_distance_moved" | "walk_distance" => num(f64::from(r.walk_position)),
            "is_moving" => Value::bool(r.walk_speed > 0.01),
            "frame_alpha" => num(f64::from(r.partial)),
            "time_stamp" => num(self.game.world.game_time as f64),
            "model_scale" => Value::Num(1.0),
            "base_swing_duration" | "modified_swing_duration" => Value::Num(0.3),
            "hurt_time" => num(f64::from(r.hurt_time)),
            _ => match m {
                Some(m) => mob_query(m, name, args, arg_num(0)),
                None => Value::ZERO,
            },
        }
    }
    fn arrow(&mut self, _target: u32, _rhs: &Expr, _scope: &mut Scope) -> Value {
        Value::ZERO
    }
}

fn mob_query(m: &Mob, name: &str, args: &[Value], axis: f32) -> Value {
    let num = |x: f64| Value::Num(x as f32);
    let pick = |x: f64, y: f64, z: f64| match axis as i32 {
        0 => x,
        1 => y,
        _ => z,
    };
    match name {
        "is_in_water" | "is_in_water_or_rain" | "is_in_contact_with_water" => Value::bool(m.fluids.in_water),
        "head_is_in_water" => Value::bool(m.fluids.eye_in_water),
        "is_in_lava" => Value::bool(m.fluids.in_lava),
        "is_on_fire" => Value::bool(m.is_on_fire()),
        "is_invisible" => Value::bool(m.invisible),
        "health" => num(f64::from(m.health)),
        "max_health" => num(f64::from(m.kind.max_health)),
        // As A&B's BedrockQueries: from the tick's movement, not velocity.
        "vertical_speed" => num((m.position.y - m.previous.y) * 20.0),
        "ground_speed" => {
            let (dx, dz) = (m.position.x - m.previous.x, m.position.z - m.previous.z);
            num((dx * dx + dz * dz).sqrt() * 20.0)
        }
        "yaw_speed" => {
            let mut d = (m.yaw - m.previous_yaw) % 360.0;
            if d >= 180.0 { d -= 360.0 }
            if d < -180.0 { d += 360.0 }
            num(f64::from(d))
        }
        "position" => num(pick(m.position.x, m.position.y, m.position.z)),
        "position_delta" => num(pick(m.position.x - m.previous.x, m.position.y - m.previous.y, m.position.z - m.previous.z)),
        "invulnerable_ticks" => num(f64::from(m.invulnerable)),
        "is_sneaking" => Value::bool(m.extra.crouching),
        "is_sprinting" => Value::bool(m.extra.sprinting),
        "is_gliding" => Value::bool(m.extra.fall_flying),
        "is_swimming" | "is_crawling" => Value::bool(matches!(m.extra.avatar_pose, horizon_client::pose::Pose::Swimming)),
        "is_sleeping" => Value::bool(matches!(m.extra.avatar_pose, horizon_client::pose::Pose::Sleeping)),
        "is_sitting" => Value::bool(m.extra.sitting || m.extra.camel_sitting),
        "is_tamed" => Value::bool(m.extra.tame),
        "is_saddled" => Value::bool(m.extra.saddled),
        "is_sheared" => Value::bool(m.extra.sheared),
        "is_chested" => Value::bool(m.extra.chested),
        "is_charged" | "is_powered" => Value::bool(m.extra.charged),
        "is_angry" => Value::bool(m.extra.angry_at.is_some()),
        "is_leashed" => Value::bool(m.extra.leash.is_some()),
        "is_riding" => Value::bool(m.extra.passenger),
        "has_target" | "is_aggressive" => Value::bool(m.ai.target.is_some() || m.ai.aggressive),
        // A slime's or magma cube's variant is its size (1, 2, 4): the pack
        // scales the model by it (`scripts.scale`).
        "variant" if m.extra.size > 0 => num(f64::from(m.extra.size)),
        "variant" => num(f64::from(m.extra.variant)),
        "mark_variant" => num(f64::from(m.extra.variant2)),
        "fuse_time" | "swelling_dir" => num(f64::from(m.swell)),
        "swell_amount" => num(if m.max_swell > 0 { f64::from(m.swell) / f64::from(m.max_swell) } else { 0.0 }),
        "is_item_name_any" | "is_item_equipped" | "has_head_gear" | "get_equipped_item_name" => {
            let stacks = m.equipment_stacks();
            let slot = args.first().and_then(Value::as_str).and_then(slot_index);
            let item = |i: usize| stacks[i].as_ref().map(|s| s.item.to_string());
            match name {
                "has_head_gear" => Value::bool(stacks[5].is_some()),
                "is_item_equipped" => Value::bool(item(if args.is_empty() { 0 } else { (args[0].num() as usize).min(1) }).is_some()),
                "get_equipped_item_name" => {
                    let i = item(if args.first().map(Value::num).unwrap_or(0.0) >= 1.0 { 1 } else { 0 }).unwrap_or_default();
                    Value::str(i.strip_prefix("minecraft:").unwrap_or(&i))
                }
                // An empty slot's name is "" (and "minecraft:air").
                _ => match slot {
                    Some(i) => {
                        let id = item(i).unwrap_or_default();
                        Value::bool(args[1..].iter().filter_map(Value::as_str).any(|n| {
                            n == id || (id.is_empty() && (n.is_empty() || n == "minecraft:air"))
                        }))
                    }
                    None => Value::ZERO,
                },
            }
        }
        _ => Value::ZERO,
    }
}

/// What both envs know about the body being drawn, for the queries they share.
struct Who<'a> {
    /// feet, interpolated
    pos: [f64; 3],
    eye: f64,
    /// absolute body and head yaw, pitch (degrees)
    body_yaw: f32,
    head_yaw: f32,
    pitch: f32,
    mob: Option<&'a Mob>,
    /// main hand, off hand
    hands: [String; 2],
    /// the hotbar, when it is this window's player
    hotbar: Option<&'a [Option<horizon_client::items::ItemStack>]>,
    using_ticks: i32,
    blocking: bool,
    /// the main hand's crossbow is loaded
    charged: bool,
}

/// `oreville:*` and `minecraft:*` item tags A&S's player reads, by item id (its data pack's
/// tags aren't in the content mod, so they are matched by name here).
fn item_tag(id: &str, tag: &str) -> bool {
    let n = id.strip_prefix("minecraft:").unwrap_or(id);
    let t = tag.split(':').nth(1).unwrap_or(tag);
    let ends = |s: &str| n.ends_with(s);
    match t {
        "is_sword" => ends("_sword"),
        "is_axe" => ends("_axe") && !ends("pickaxe"),
        "is_pickaxe" => ends("_pickaxe"),
        "is_shovel" => ends("_shovel"),
        "is_hoe" => ends("_hoe"),
        "is_spear" => ends("_spear"),
        "is_trident" => n == "trident",
        "is_mace" => n == "mace",
        "is_shield" => n == "shield",
        "is_fishing_rod" => n == "fishing_rod",
        "is_rod" => n == "carrot_on_a_stick" || n == "warped_fungus_on_a_stick" || n == "fishing_rod",
        "is_crossbow" => n == "crossbow",
        "is_bow" => n == "bow",
        "is_golden_dandelion" => n == "golden_dandelion",
        "is_shears" => n == "shears",
        "is_flint_and_steel" => n == "flint_and_steel" || n == "fire_charge",
        "is_cushion" => ends("_cushion"),
        "is_throwable" => matches!(n, "snowball" | "egg" | "blue_egg" | "brown_egg" | "ender_pearl" | "splash_potion" | "lingering_potion" | "experience_bottle" | "wind_charge" | "ender_eye"),
        "is_readable" => matches!(n, "book" | "written_book" | "writable_book" | "enchanted_book" | "knowledge_book"),
        "is_torch" => matches!(n, "torch" | "soul_torch" | "redstone_torch" | "copper_torch"),
        "is_petals" => matches!(n, "pink_petals" | "wildflowers" | "leaf_litter"),
        "is_seeds" => ends("_seeds") || n == "pitcher_pod",
        "is_brush" => n == "brush",
        "is_horn" => n == "goat_horn",
        "is_spyglass" => n == "spyglass",
        "is_potion" => ends("potion"),
        "is_drink" => n == "potion" || n == "milk_bucket" || n == "honey_bottle" || n == "ominous_bottle",
        "is_milk" => n == "milk_bucket",
        "is_soup" => ends("_stew") || n == "beetroot_soup",
        "is_upturned" => matches!(n, "nether_star" | "end_crystal" | "heart_of_the_sea" | "conduit"),
        "music_disc" => n.starts_with("music_disc_"),
        "is_tool" => ["_pickaxe", "_axe", "_shovel", "_hoe", "_sword", "_spear"].iter().any(|e| ends(e)) || matches!(n, "shears" | "mace" | "trident"),
        "is_armor" => ["_helmet", "_chestplate", "_leggings", "_boots"].iter().any(|e| ends(e)),
        "is_food" => matches!(n, "apple" | "golden_apple" | "enchanted_golden_apple" | "bread" | "cookie" | "carrot" | "golden_carrot" | "potato" | "baked_potato" | "poisonous_potato" | "beetroot" | "melon_slice" | "sweet_berries" | "glow_berries" | "chorus_fruit" | "dried_kelp" | "pumpkin_pie" | "cake" | "honey_bottle" | "rotten_flesh" | "spider_eye")
            || ["beef", "porkchop", "chicken", "mutton", "rabbit", "cod", "salmon", "tropical_fish", "pufferfish", "_stew", "_soup"].iter().any(|e| ends(e)),
        "harness" => ends("_harness"),
        "sign" => ends("_sign") && !ends("_hanging_sign"),
        "hanging_sign" => ends("_hanging_sign"),
        "wooden_tier" => n.starts_with("wooden_"),
        "stone_tier" => n.starts_with("stone_"),
        "copper_tier" => n.starts_with("copper_"),
        "iron_tier" => n.starts_with("iron_"),
        "golden_tier" => n.starts_with("golden_"),
        "diamond_tier" => n.starts_with("diamond_"),
        "netherite_tier" => n.starts_with("netherite_"),
        "leather_tier" => n.starts_with("leather_"),
        "chainmail_tier" => n.starts_with("chainmail_"),
        _ => false,
    }
}

fn mc_yaw(dx: f64, dz: f64) -> f64 {
    (-dx).atan2(dz).to_degrees()
}

/// This window's player's worn armour (`game.armor`: head, chest, legs, feet)
/// for `q.is_item_name_any('slot.armor.*', ...)` and `q.has_head_gear`: its
/// model in Horizon carries no equipment.
fn local_armor_query(game: &super::Game<'_>, name: &str, args: &[Value]) -> Option<Value> {
    let worn = |i: usize| game.armor.get(i).and_then(|s| s.as_ref()).map(|s| s.item.to_string()).unwrap_or_default();
    match name {
        "has_head_gear" => Some(Value::bool(!worn(0).is_empty())),
        "is_item_name_any" => {
            let i = match args.first().and_then(Value::as_str)? {
                "slot.armor.head" => 0,
                "slot.armor.chest" => 1,
                "slot.armor.legs" => 2,
                "slot.armor.feet" => 3,
                _ => return None,
            };
            Some(Value::bool(names_match(&worn(i), &args[1..])))
        }
        _ => None,
    }
}

fn names_match(id: &str, names: &[Value]) -> bool {
    names.iter().filter_map(Value::as_str).any(|n| n == id || (id.is_empty() && (n.is_empty() || n == "minecraft:air")))
}

/// The queries the third- and first-person envs answer the same way; None hands the
/// query on.
fn shared_query(game: &super::Game<'_>, w: &Who<'_>, name: &str, args: &[Value]) -> Option<Value> {
    let num = |x: f64| Value::Num(x as f32);
    let arg = |i: usize| args.get(i).map(Value::num).unwrap_or(0.0);
    let cam = game.native_camera();
    Some(match name {
        "ans_expression_skin" => Value::bool(SKIN_OK.with(std::cell::Cell::get)),
        "body_y_rotation" => num(f64::from(w.body_yaw)),
        // The movement scripts' source of speed and direction (BedrockQueries
        // `position`): the body's tick position, for the local player too.
        "position" if w.mob.is_some() => {
            let m = w.mob.unwrap();
            num([m.position.x, m.position.y, m.position.z][(arg(0) as usize).min(2)])
        }
        "position_delta" if w.mob.is_some() => {
            let m = w.mob.unwrap();
            num([m.position.x - m.previous.x, m.position.y - m.previous.y, m.position.z - m.previous.z][(arg(0) as usize).min(2)])
        }
        // A&B's own (the jar's): the entity's position at this tick, not
        // interpolated, per axis. The movement scripts take speed and
        // direction from its change between ticks.
        "ans_tick_position" => {
            let p = w.mob.map_or(w.pos, |m| [m.position.x, m.position.y, m.position.z]);
            num(p[(arg(0) as usize).min(2)])
        }
        // A&B's first person arm inertia option (on in the jar by default).
        "ans_arm_inertia" => Value::bool(true),
        // The custom enchant outline colour (r, g, b): white.
        "ans_outline_color" => num(1.0),
        // Bedrock: midnight 0, sunrise 0.25, noon 0.5 (Java's tick 0 is sunrise).
        "time_of_day" => num((game.world.day_time.rem_euclid(24000) as f64 / 24000.0 + 0.25).fract()),
        "has_cape" | "is_shaking" | "equipment_count" | "cooldown_time" | "cooldown_time_remaining" => Value::ZERO,
        "head_y_rotation" => num(f64::from(w.head_yaw)),
        "head_x_rotation" | "target_x_rotation" => num(f64::from(w.pitch)),
        "target_y_rotation" => {
            let mut d = (w.head_yaw - w.body_yaw) % 360.0;
            if d >= 180.0 {
                d -= 360.0;
            }
            if d < -180.0 {
                d += 360.0;
            }
            num(f64::from(d))
        }
        "camera_rotation" => {
            let c = cam?;
            num(if arg(0) < 0.5 { f64::from(c.pitch) } else { f64::from(c.yaw) })
        }
        "distance_from_camera" => {
            let c = cam?;
            let d = [c.position[0] - w.pos[0], c.position[1] - (w.pos[1] + w.eye), c.position[2] - w.pos[2]];
            num((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
        }
        "rotation_to_camera" => {
            let c = cam?;
            let d = [c.position[0] - w.pos[0], c.position[1] - (w.pos[1] + w.eye), c.position[2] - w.pos[2]];
            if arg(0) < 0.5 {
                num(-(d[1].atan2(d[0].hypot(d[2]))).to_degrees())
            } else {
                num(mc_yaw(d[0], d[2]))
            }
        }
        "equipped_item_any_tag" => {
            let i = if args.first().and_then(Value::as_str) == Some("slot.weapon.offhand") { 1 } else { 0 };
            let id = &w.hands[i];
            Value::bool(!id.is_empty() && args[1..].iter().filter_map(Value::as_str).any(|t| item_tag(id, t)))
        }
        "is_item_name_any" if args.first().and_then(Value::as_str) == Some("slot.hotbar") => {
            let slot = arg(1) as usize;
            let id = w.hotbar.and_then(|h| h.get(slot)).and_then(|s| s.as_ref()).map(|s| s.item.to_string()).unwrap_or_default();
            Value::bool(names_match(&id, &args[2..]))
        }
        "is_item_name_any" if matches!(args.first().and_then(Value::as_str), Some("slot.weapon.mainhand" | "slot.weapon.offhand")) => {
            let i = usize::from(args.first().and_then(Value::as_str) == Some("slot.weapon.offhand"));
            let names = if args.get(1).is_some_and(|a| a.as_str().is_none()) { &args[2..] } else { &args[1..] };
            Value::bool(names_match(&w.hands[i], names))
        }
        "get_equipped_item_name" => {
            let i = usize::from(arg(0) >= 1.0 || args.first().and_then(Value::as_str) == Some("off_hand"));
            Value::str(w.hands[i].strip_prefix("minecraft:").unwrap_or(&w.hands[i]))
        }
        "is_item_equipped" => Value::bool(!w.hands[usize::from(arg(0) >= 1.0)].is_empty()),
        "main_hand_item_use_duration" => num(f64::from(w.using_ticks) / 20.0),
        "main_hand_item_max_duration" => num(if w.using_ticks > 0 { 72000.0 / 20.0 } else { 0.0 }),
        "is_using_item" => Value::bool(w.using_ticks > 0),
        "blocking" => Value::bool(w.blocking),
        "item_is_charged" => Value::bool(w.charged),
        "is_riding_any_entity_of_type" => {
            let Some(m) = w.mob else { return Some(Value::ZERO) };
            let Some(v) = m.extra.mount.and_then(|v| game.mobs.iter().find(|x| x.id == v)) else { return Some(Value::ZERO) };
            Value::bool(args.iter().filter_map(Value::as_str).any(|k| *v.kind.id == *k || (k.ends_with("boat") && v.kind.id.ends_with("boat"))))
        }
        "ride_body_y_rotation" => {
            let v = w.mob.and_then(|m| m.extra.mount).and_then(|v| game.mobs.iter().find(|x| x.id == v));
            num(v.map_or(0.0, |v| f64::from(v.body_yaw)))
        }
        "relative_block_has_any_tag" | "relative_block_has_all_tags" => {
            let yaw = f64::from(w.body_yaw).to_radians();
            let (sin, cos) = (yaw.sin(), yaw.cos());
            let (x, y, z) = (f64::from(arg(0)), f64::from(arg(1)), f64::from(arg(2)));
            let p = [w.pos[0] + x * cos - z * sin, w.pos[1] + y, w.pos[2] + x * sin + z * cos];
            let b = game.native_block([p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32]);
            let tags: Vec<&str> = args[3..].iter().filter_map(Value::as_str).collect();
            let has = |t: &str| match t {
                "water" => b.contains("water") || b == "minecraft:seagrass" || b == "minecraft:kelp" || b == "minecraft:bubble_column",
                "lava" => b.contains("lava"),
                "air" => b.ends_with("air"),
                "solid" => !b.ends_with("air") && !b.contains("water") && !b.contains("lava"),
                other => b.ends_with(other),
            };
            Value::bool(if name.ends_with("any_tag") { tags.iter().any(|t| has(t)) } else { tags.iter().all(|t| has(t)) })
        }
        "has_armor_slot" => {
            let Some(m) = w.mob else { return Some(Value::ZERO) };
            Value::bool(m.extra.armor.get(3usize.saturating_sub(arg(0) as usize)).is_some_and(Option::is_some))
        }
        "ans_at_water_surface" => {
            let feet = game.native_block([w.pos[0].floor() as i32, (w.pos[1] + 0.5).floor() as i32, w.pos[2].floor() as i32]);
            let head = game.native_block([w.pos[0].floor() as i32, (w.pos[1] + w.eye).floor() as i32, w.pos[2].floor() as i32]);
            Value::bool(feet.contains("water") && !head.contains("water"))
        }
        "movement_direction" => {
            let Some(m) = w.mob else { return Some(Value::ZERO) };
            let (dx, dz) = (m.position.x - m.previous.x, m.position.z - m.previous.z);
            let l = dx.hypot(dz);
            if l < 1e-4 {
                Value::ZERO
            } else {
                num(match arg(0) as i32 {
                    0 => dx / l,
                    2 => dz / l,
                    _ => 0.0,
                })
            }
        }
        "is_local_player" => Value::bool(w.hotbar.is_some()),
        _ => return None,
    })
}

fn mob_by_id<'a>(game: &'a super::Game<'_>, id: u32) -> Option<&'a Mob> {
    game.mobs.iter().find(|m| m.id == id && !m.removed)
}

fn load_settings(game: &super::Game<'_>, host: &mut Host) {
    SETTINGS_READ.with(|r| {
        if *r.borrow() {
            return;
        }
        *r.borrow_mut() = true;
        for d in host.settings.defs.clone() {
            if let Some(v) = game.native_setting(ID, &setting_key(&d.id)) {
                host.settings.chosen.insert(d.id.clone(), v);
            }
        }
    });
    apply_txt(host);
}

/// `config/ans_beyond.txt`: the player's own settings, as `key = value` lines
/// (`#` starts a comment). Our own keys first, then every pack setting from
/// `_settings.json` by id, which wins over the settings screen whenever the
/// file is saved. Written with the defaults when missing; read again when it
/// changes (checked about once a second).
#[derive(Clone)]
struct Cfg {
    held_items: bool,
    first_person_items_follow_arm: bool,
    item_pitch: f32,
    item_offset: [f32; 3],
    fp_arm_offset: [f32; 3],
    armor: bool,
    attachables: bool,
    /// First person arm size (1 = the jar's), about the shoulders.
    fp_arm_scale: f32,
    /// First person size of 3D held items (1 = the jar's), about the hand.
    fp_item_scale: f32,
    /// The enchantment outline around enchanted held items.
    outline: bool,
    /// Its colour, an index into `OUTLINE_COLORS` (the last one cycles).
    outline_color: f32,
    /// Its width, a share of the item's size.
    outline_width: f32,
    /// 0 glint (the game's shimmer on the outline), 1 solid colour.
    outline_style: f32,
    /// The item keeps its own glint under the outline.
    outline_item_glint: bool,
    /// The key that opens the settings screen, by its vanilla name.
    menu_key: String,
    pack: Vec<(String, String)>,
}

impl Default for Cfg {
    fn default() -> Cfg {
        Cfg { held_items: true, first_person_items_follow_arm: false, item_pitch: 180.0, item_offset: [0.0, 0.0, 0.0], fp_arm_offset: [-0.12, 0.2, 0.0], armor: true, attachables: true, fp_arm_scale: 1.0, fp_item_scale: 0.8, outline: true, outline_color: 0.0, outline_width: 0.02, outline_style: 0.0, outline_item_glint: false, menu_key: "key.keyboard.k".into(), pack: Vec::new() }
    }
}

thread_local! {
    static CFG: RefCell<(Cfg, Option<std::time::SystemTime>, u32, bool)> = RefCell::new((Cfg::default(), None, 0, false));
}

fn config_path() -> Option<std::path::PathBuf> {
    super::Game::native_config_dir().ok().map(|d| d.join("ans_beyond.txt"))
}

fn config() -> Cfg {
    CFG.with(|c| c.borrow().0.clone())
}

fn parse_cfg(text: &str) -> Cfg {
    let mut c = Cfg::default();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        let b = |v: &str| matches!(v.to_ascii_lowercase().as_str(), "true" | "on" | "yes" | "1");
        let f = |v: &str, d: f32| v.parse::<f32>().unwrap_or(d);
        match k {
            "held_items" => c.held_items = b(v),
            "first_person_items_follow_arm" => c.first_person_items_follow_arm = b(v),
            "item_pitch" => c.item_pitch = f(v, 180.0),
            "item_offset_x" => c.item_offset[0] = f(v, 0.0),
            "item_offset_y" => c.item_offset[1] = f(v, 0.0),
            "item_offset_z" => c.item_offset[2] = f(v, 0.0),
            "armor" => c.armor = b(v),
            "attachables" => c.attachables = b(v),
            "fp_arm_offset_x" => c.fp_arm_offset[0] = f(v, -0.12),
            "fp_arm_offset_y" => c.fp_arm_offset[1] = f(v, 0.2),
            "fp_arm_offset_z" => c.fp_arm_offset[2] = f(v, 0.0),
            "fp_arm_scale" => c.fp_arm_scale = f(v, 1.0).clamp(0.3, 2.0),
            "fp_item_scale" => c.fp_item_scale = f(v, 0.8).clamp(0.3, 2.0),
            "enchant_outline" => c.outline = b(v),
            "enchant_outline_color" => c.outline_color = f(v, 0.0).round().clamp(0.0, (OUTLINE_COLORS.len() - 1) as f32),
            "enchant_outline_width" => c.outline_width = f(v, 0.02).clamp(0.005, 0.1),
            "enchant_outline_style" => c.outline_style = f(v, 0.0).round().clamp(0.0, 1.0),
            "enchant_outline_item_glint" => c.outline_item_glint = b(v),
            "menu_key" => c.menu_key = v.to_string(),
            "config_version" => {}
            _ if !v.is_empty() => c.pack.push((k.to_string(), v.to_string())),
            _ => {}
        }
    }
    c
}

fn write_default_cfg(path: &std::path::Path, host: &Host) {
    let mut t = String::from(
        "# Actions & Beyond for Horizon: your settings. Edit and save; the game picks it up within a second.\n\
         # Delete this file to get the defaults back.\n\n\
         # Held items drawn in A&B's hands (third person, and first person when the next is on).\n\
         config_version = 5\n\
         held_items = true\n\
         # Worn armour drawn on A&B's model (also off with the pack's armor_visibility).\n\
         armor = true\n\
         # Held items as the jar draws them: its 3D models (attachables), with the first person arm holding them.\n\
         attachables = true\n\
         # Off (the jar): an item without an attachable stays where vanilla holds it. On: it is drawn at A&B's hand.\n\
         first_person_items_follow_arm = false\n\
         # How the item sits in the hand: a turn in degrees about the hand's x axis, then an offset in blocks.\n\
         item_pitch = 180\n\
         item_offset_x = 0\n\
         item_offset_y = 0\n\
         item_offset_z = 0\n\
         # Moves A&B's first person arms: blocks right, up and back from where the jar puts them (0 0 0 = the jar, which leaves the empty arm mostly off screen).\n\
         fp_arm_offset_x = -0.12\n\
         fp_arm_offset_y = 0.2\n\
         fp_arm_offset_z = 0\n\
         # First person arm size (1 = the jar's).\n\
         fp_arm_scale = 1\n\
         # First person size of 3D held items, about the hand holding them (1 = the jar's).\n\
         fp_item_scale = 0.8\n\
         # Enchantment Glint Outline (CC0) around enchanted held items: style (0 glint, 1 solid colour), colour for solid (0 lilac, 1 blue, 2 aqua, 3 green, 4 yellow, 5 orange, 6 red, 7 pink, 8 white, 9 rainbow), width (a share of the item's size), and whether the item keeps its own glint.\n\
         enchant_outline = true\n\
         enchant_outline_style = 0\n\
         enchant_outline_color = 0\n\
         enchant_outline_width = 0.02\n\
         enchant_outline_item_glint = false\n\
         # The key that opens the Actions & Stuff screen (vanilla key names: key.keyboard.k, key.keyboard.f8...).\n\
         menu_key = key.keyboard.k\n\n\
         # The pack's own settings (the same as Options > Mods > Actions & Beyond).\n\
         # Leave a value empty to use the settings screen's.\n",
    );
    for d in &host.settings.defs {
        let opts = if d.toggle { "true / false".to_string() } else { d.options.join(" / ") };
        t.push_str(&format!("\n# {} ({})\n{} = \n", d.readable, opts, d.id));
    }
    let _ = std::fs::write(path, t);
}

fn apply_txt(host: &mut Host) {
    CFG.with(|c| {
        let mut c = c.borrow_mut();
        c.2 = c.2.wrapping_add(1);
        if c.3 && c.2 % 60 != 1 {
            return;
        }
        let Some(path) = config_path() else { return };
        // A file from an older build (no `config_version = 5`) gets the new defaults.
        let old = std::fs::read_to_string(&path).is_ok_and(|t| !t.contains("config_version = 5"));
        if (old || !path.exists()) && !host.settings.defs.is_empty() {
            write_default_cfg(&path, host);
        }
        let stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if stamp == c.1 && c.3 {
            return;
        }
        c.1 = stamp;
        c.3 = true;
        let cfg = std::fs::read_to_string(&path).map(|t| parse_cfg(&t)).unwrap_or_default();
        for (k, v) in &cfg.pack {
            if let Some(d) = host.settings.defs.iter().find(|d| d.id == *k || setting_key(&d.id) == *k) {
                host.settings.chosen.insert(d.id.clone(), v.clone());
            }
        }
        c.0 = cfg;
    });
}

fn setting_key(id: &str) -> String {
    id.replace(':', "_")
}

/// `native_mob_submit`: draws `render`'s mob with its Bedrock model when the
/// content mod has a bundle for its type.
/// The game's pixel sizes of the sprites the jar's composed textures copy
/// from, until every one is known (the atlas can come after the first mob).
thread_local! {
    /// Every composed texture's sprite sizes are known (until packs reload).
    static SIZES_KNOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn composite_sizes(game: &super::Game<'_>, host: &mut Host) {
    // Every mob's draw asks: once all are known, nothing to do.
    if SIZES_KNOWN.with(|k| k.get()) {
        return;
    }
    if host.composite_sizes_known() && !host.sprite_sizes.is_empty() {
        SIZES_KNOWN.with(|k| k.set(true));
        return;
    }
    for s in host.composite_sprites() {
        if let Some(size) = game.native_sprite_size(&s) {
            host.sprite_sizes.insert(s, size);
        }
    }
}

/// `AnimatedMob.changeColor`: what the jar tints a mob's composed texture by,
/// a sheep's wool (`jeb_`'s cycling) and a cat's or wolf's collar (0 none).
fn change_color(mob: Option<&Mob>, kind: &str, age: f32) -> u32 {
    use horizon_client::mobs::render::{dye, jeb_wool_rgb, sheep_wool_rgb};
    let Some(m) = mob else { return 0 };
    match kind.trim_start_matches("minecraft:") {
        "sheep" if m.custom_name.as_deref() == Some("jeb_") => jeb_wool_rgb(age),
        "sheep" => sheep_wool_rgb(m.extra.color as _),
        "cat" | "wolf" => dye(m.extra.color as usize) & 0xFF_FFFF,
        _ => 0,
    }
}

pub fn draw_mob(
    game: &super::Game<'_>,
    render: &super::NativeMobRender<'_>,
    draw: &mut super::BlockEntityDraw,
    drawn: &mut bool,
) {
    if *drawn || !game.native_on(ID) {
        return;
    }
    let mob = mob_by_id(game, render.id);
    draw_with(game, render, mob, None, false, draw, drawn);
}

fn draw_with(
    game: &super::Game<'_>,
    render: &super::NativeMobRender<'_>,
    mob: Option<&Mob>,
    skin: Option<&str>,
    local: bool,
    draw: &mut super::BlockEntityDraw,
    drawn: &mut bool,
) {
    let src = Resources(game.native_client_resources());
    let a = f64::from(render.partial);
    let pos = mob.map_or([0.0; 3], |m| {
        let p = m.previous.lerp(m.position, a);
        [p.x, p.y, p.z]
    });
    let stacks = mob.map(|m| m.equipment_stacks());
    let hand = |i: usize| stacks.as_ref().and_then(|s| s[i].as_ref()).map(|s| s.item.to_string()).unwrap_or_default();
    let (hands, using_ticks, blocking) = if local {
        let items = game.visible_hand_items();
        let id = |i: usize| items[i].as_ref().map(|s| s.item.to_string()).unwrap_or_default();
        let pm = game.player_model.as_ref();
        ([id(0), id(1)], pm.map_or(0, |m| m.extra.using_item), pm.is_some_and(|m| m.extra.blocking))
    } else {
        ([hand(0), hand(1)], mob.map_or(0, |m| m.extra.using_item), mob.is_some_and(|m| m.extra.blocking))
    };
    let who = Who {
        pos,
        eye: mob.map_or(1.62, |m| m.kind.eye_height),
        body_yaw: render.body_yaw,
        head_yaw: render.body_yaw + render.head_yaw,
        pitch: render.pitch,
        mob,
        hands,
        hotbar: local.then(|| &game.inventory[..9.min(game.inventory.len())]),
        using_ticks,
        blocking,
        charged: stacks.as_ref().and_then(|s| s[0].as_ref()).and_then(|s| s.data()).is_some_and(|d| !d.charged.is_empty())
            || (local && game.visible_hand_items()[0].as_ref().and_then(|s| s.data()).is_some_and(|d| !d.charged.is_empty())),
    };
    // The swing (`attack_time`, 0 to 1): this window's from its hand, others' from their mob.
    let swing = if local {
        game.swing.animation(render.partial)
    } else {
        mob.map_or(0.0, |m| if m.ai.swinging && m.ai.swing_duration > 0 { (m.ai.swing_time as f32 + render.partial) / m.ai.swing_duration as f32 } else { 0.0 })
    };
    let held_stacks: [Option<ItemStack>; 2] = if local {
        game.visible_hand_items()
    } else {
        stacks.as_ref().map_or([None, None], |s| [s[0].clone(), s[1].clone()])
    };
    let (fall_flying, velocity) = if local && let Some(p) = game.player.as_ref() {
        (p.fall_flying, [p.velocity.x, p.velocity.y, p.velocity.z])
    } else {
        mob.map_or((false, [0.0; 3]), |m| (m.extra.fall_flying, [m.velocity.x, m.velocity.y, m.velocity.z]))
    };
    // The elytra is drawn by the jar's attachable for the chest slot.
    let chest_elytra = if local {
        game.armor.get(1).and_then(|s| s.as_ref()).is_some_and(|s| &*s.item == "minecraft:elytra")
    } else {
        stacks.as_ref().and_then(|s| s[4].as_ref()).is_some_and(|s| &*s.item == "minecraft:elytra")
    };
    let mut vars = vec![("is_first_person", 0.0), ("is_paperdoll", 0.0), ("map_face_icon", 0.0), ("bob_animation", 1.0), ("attack_time", swing.clamp(0.0, 1.0))];
    vars.extend(player_vars(game, held_stacks[0].as_ref(), held_stacks[1].as_ref(), render.pitch, fall_flying, velocity, who.using_ticks));
    let context = [("player_arm_height", 1.0), ("player_offhand_arm_height", 1.0)];
    HOST.with(|h| {
        let mut host = h.borrow_mut();
        host.load_settings(&src);
        load_settings(game, &mut host);
        if !host.handles(&src, render.kind) {
            return;
        }
        host.set_attachables(config().attachables);
        let held_ids = who.hands.clone();
        let frame = MobFrame {
            id: render.id,
            kind: render.kind,
            age_ticks: f64::from(render.age),
            hurt: render.hurt_time > 0,
            on_fire: mob.is_some_and(|m| m.is_on_fire()),
            skin,
            vars: &vars,
            context: &context,
            held: [held_ids[0].as_str(), held_ids[1].as_str()],
            worn: ["", if chest_elytra { "minecraft:elytra" } else { "" }, "", ""],
        };
        SKIN_OK.with(|c| c.set(skin.is_some_and(expression_skin)));
        composite_sizes(game, &mut host);
        host.change_color = change_color(mob, render.kind, render.age);
        let mut env = MobEnv { game, mob, render, who };
        if !host.draw_mob(&src, &frame, &mut env) || host.quads.is_empty() {
            // Nothing to draw (a model made only of glow layers): keep vanilla's.
            return;
        }
        if local {
            trace(game, &host, render.id, &env.who, "third person");
        }
        drop_undrawable(game, &mut host);
        // Worn armour (the jar's EquipmentLayer): the game draws none for a
        // player a mod draws. A morph wears it too, on whichever of the head,
        // body, arm and leg bones its model has (the jar's layer has no morph
        // check).
        if config().armor && host.settings.value("oreville_ans:cvmbbr") != Some("disabled") {
            let worn: [Option<ItemStack>; 4] = if local {
                game.armor.clone()
            } else {
                stacks.as_ref().map_or(Default::default(), |s| [s[5].clone(), s[4].clone(), s[3].clone(), s[2].clone()])
            };
            let pieces: Vec<render::ArmorPiece> = worn
                .iter()
                .enumerate()
                .filter_map(|(slot, st)| {
                    let st = st.as_ref()?;
                    let asset = game.res.items().get(&st.item).and_then(|i| i.equip_asset.clone())?;
                    let (ns, name) = asset.split_once(':').unwrap_or(("minecraft", &asset));
                    let kind = if slot == 2 { "humanoid_leggings" } else { "humanoid" };
                    let dyed = st.data().and_then(|d| d.dyed_color).map(|c| c as u32 & 0xFF_FFFF);
                    let tint = if name == "leather" { dyed.unwrap_or(0xA0_6540) } else { 0xFF_FFFF };
                    Some(render::ArmorPiece { slot, texture: format!("{ns}:textures/entity/equipment/{kind}/{name}.png"), tint })
                })
                .collect();
            host.armor(&pieces);
        }
        // Gliding: the jar's PlayerRenderer.applyRotations tips the body
        // (XP by fallFlyingScale * (-90 - xRot), vanilla's AvatarRenderer).
        // The game's pose here faces +z where vanilla's faces -z, so the
        // angle's sign turns.
        if let Some(m) = mob
            && m.extra.fall_flying
            && !m.extra.auto_spin_attack
            && render.death_time == 0
        {
            let t = m.extra.fall_fly_ticks as f32 + render.partial;
            let scale = (t * t / 100.0).clamp(0.0, 1.0);
            draw.rotate_about([1.0, 0.0, 0.0], scale * (90.0 + render.pitch), [0.0; 3]);
        }
        let light = draw.light;
        let held: [Option<ItemStack>; 2] = if local {
            game.visible_hand_items()
        } else {
            stacks.as_ref().map_or([None, None], |s| [s[0].clone(), s[1].clone()])
        };
        let foils = [0, 1].map(|i| foil(game, held[i].as_ref()));
        let mut glint: Vec<(usize, (usize, usize))> = (0..2).filter(|&i| foils[i]).map(|i| (i, host.att_quads[i])).collect();
        // The model's own held armour piece (main hand).
        let model_held = host.held_model.1 > host.held_model.0;
        if foils[0] && model_held {
            glint.push((0, host.held_model));
        }
        let mut spans = draw_quads(game, draw, &host.quads, light, &glint);
        if config().held_items {
            let mut hands = host.hands;
            // A hand whose item the jar's attachable drew draws nothing more,
            // nor one the model's own held armour piece is in.
            for (slot, h) in hands.iter_mut().enumerate() {
                if host.drew_attachable(render.id, slot) || (slot == 0 && model_held) {
                    *h = None;
                }
            }
            let items = draw_hand_items(game, draw, hands, [held[0].as_ref(), held[1].as_ref()], false, render.id as i32);
            for i in 0..2 {
                if spans[i].is_none() {
                    spans[i] = items[i];
                }
            }
        }
        for i in 0..2 {
            if let (true, Some((a, b))) = (foils[i], spans[i]) {
                outline(game, draw.vertices_mut(), a, b);
            }
        }
        *drawn = true;
    });
}

/// Whether `stack` has the enchantment glint (`ItemStack.hasFoil`).
fn foil(game: &super::Game<'_>, stack: Option<&ItemStack>) -> bool {
    stack.is_some_and(|s| s.has_foil(game.res.items().enchantments.props.get(&*s.item).is_some_and(|p| p.glint)))
}

/// Draws `quads` (full bright ones at full light, the rest at `light`); the
/// quads in a `glint` range (a held enchanted item's 3D model) with the
/// game's enchantment glint, as vanilla's foil items: the pack's own glint
/// layers are additive, which this drawing can't blend.
fn draw_quads(game: &super::Game<'_>, draw: &mut super::BlockEntityDraw, quads: &[render::Quad], light: u32, glint: &[(usize, (usize, usize))]) -> [Option<(usize, usize)>; 2] {
    let g = game.glint();
    let mut spans: [Option<(usize, usize)>; 2] = [None, None];
    for (i, q) in quads.iter().enumerate() {
        draw.light = if q.full_bright { FULL_BRIGHT } else { light };
        let before = draw.vertices_mut().len();
        if !game.native_draw_quad(draw, &q.sprite, q.corners, q.uv, q.tint) {
            continue;
        }
        let Some(&(hand, _)) = glint.iter().find(|&&(_, (a, b))| i >= a && i < b) else { continue };
        let after = draw.vertices_mut().len();
        let s = &mut spans[hand.min(1)];
        *s = Some(s.map_or((before, after), |(a, _)| (a, after)));
        let Some(g) = g else { continue };
        for (k, v) in draw.vertices_mut()[before..].iter_mut().enumerate() {
            // Overlay 7: the entity shader's glint over the texture.
            if v.layer & 0xFFFF == 0 {
                v.layer |= 7;
            }
            let c = q.corners[k.min(3)];
            v.glint_uv = g.uv([(c[0] + c[2]) * 2.0, c[1] * 2.0]);
        }
    }
    draw.light = light;
    spans
}

/// The enchantment outline's colours (`enchant_outline_color`), the last one
/// cycling through the others.
const OUTLINE_COLORS: [(&str, u32); 10] = [
    ("Lilac", 0xD296F8),
    ("Blue", 0x4D9BFF),
    ("Aqua", 0x4DFFF0),
    ("Green", 0x5CFF4D),
    ("Yellow", 0xFFE14D),
    ("Orange", 0xFF8A2E),
    ("Red", 0xFF4D4D),
    ("Pink", 0xFF6BD5),
    ("White", 0xFFFFFF),
    ("Rainbow", 0),
];

/// The outline colour now (0xRRGGBB): rainbow cycles every 0.6 s.
fn outline_rgb(game: &super::Game<'_>) -> u32 {
    let i = config().outline_color.round() as usize;
    if i + 1 < OUTLINE_COLORS.len() {
        return OUTLINE_COLORS[i].1;
    }
    // Rainbow: the colours before white, one into the next.
    let n = OUTLINE_COLORS.len() - 2;
    let t = game.world.game_time as f32 / 12.0;
    let k = t.floor() as usize;
    let (a, b) = (OUTLINE_COLORS[k % n].1, OUTLINE_COLORS[(k + 1) % n].1);
    let f = t - t.floor();
    let ch = |s: u32| (((a >> s & 255) as f32 * (1.0 - f) + (b >> s & 255) as f32 * f) as u32) << s;
    ch(16) | ch(8) | ch(0)
}

/// The enchantment outline around `verts[a..b]` (an enchanted item's quads,
/// four vertices each, the camera at the origin), as Enchantment Glint
/// Outline draws it (CC0, `QuadHelper.thickenQuad`): each face copied four
/// times, shifted along its two diagonals by the outline size and lifted
/// along its normal by as much, the order reversed so only the far side's
/// copies face the camera. They are also pushed back along the view, past
/// the lift, so the item covers them everywhere but its rim. "Glint" (the
/// mod's default): the copies dark with the game's glint over them; "solid":
/// in the outline colour. With `enchant_outline_item_glint` off (the mod's
/// `removeRenderPass`), the item itself loses its glint: the outline shows it.
fn outline(game: &super::Game<'_>, verts: &mut Vec<horizon_client::mesher::Vertex>, a: usize, b: usize) {
    let cfg = config();
    if !cfg.outline || b <= a || b > verts.len() {
        return;
    }
    let b = a + (b - a) / 4 * 4;
    // The item's size, for the outline's width (the mod's 20/1000 of a block
    // for a 1 block item model).
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for v in &verts[a..b] {
        for k in 0..3 {
            lo[k] = lo[k].min(v.pos[k]);
            hi[k] = hi[k].max(v.pos[k]);
        }
    }
    let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max).max(1e-4);
    let d = cfg.outline_width * size;
    let solid = cfg.outline_style >= 0.5;
    let rgb = if solid { outline_rgb(game) } else { 0x2A2A2A };
    let color = 0xFF00_0000 | (rgb & 255) << 16 | (rgb >> 8 & 255) << 8 | (rgb >> 16 & 255);
    let g = game.glint();
    let sub = |x: [f32; 3], y: [f32; 3]| [x[0] - y[0], x[1] - y[1], x[2] - y[2]];
    let norm = |x: [f32; 3]| {
        let l = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
        if l > 1e-9 { [x[0] / l, x[1] / l, x[2] / l] } else { [0.0; 3] }
    };
    let dot = |x: [f32; 3], y: [f32; 3]| x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
    let mut out = Vec::with_capacity((b - a) * 4);
    let len = |x: [f32; 3]| (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
    for q in (a..b).step_by(4) {
        let v = [verts[q], verts[q + 1], verts[q + 2], verts[q + 3]];
        let p = v.map(|x| x.pos);
        // A flat item's one pixel edge strips: their copies stick out as
        // fins (seen in game on a held sword); the faces outline it.
        if len(sub(p[1], p[0])).max(len(sub(p[2], p[1]))) < size / 10.0 {
            continue;
        }
        let c: [f32; 3] = std::array::from_fn(|k| (p[0][k] + p[1][k] + p[2][k] + p[3][k]) / 4.0);
        let (c1, c2) = (sub(p[0], c), sub(p[1], c));
        let side1 = norm([c1[0] + c2[0], c1[1] + c2[1], c1[2] + c2[2]]);
        let side2 = norm(sub(c1, c2));
        let diag = [(side1[0] + side2[0]) * d, (side1[1] + side2[1]) * d, (side1[2] + side2[2]) * d];
        let k2 = 2.0 * dot(diag, side2);
        let other = [diag[0] - k2 * side2[0], diag[1] - k2 * side2[1], diag[2] - k2 * side2[2]];
        let e1 = sub(p[1], p[0]);
        let e2 = sub(p[2], p[0]);
        let n = norm([e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]]);
        // Only a hair off the face: lifting by the outline size moved the
        // back face's copy sideways into a band beside the item.
        let lift = 1e-4;
        for dir in [diag, other, diag.map(|x| -x), other.map(|x| -x)] {
            for &k in &[3usize, 2, 1, 0] {
                let mut o = v[k];
                let mut pos: [f32; 3] = std::array::from_fn(|i| p[k][i] + dir[i] + n[i] * lift);
                // Behind the item along the view by more than the lift.
                let dist = dot(pos, pos).sqrt().max(1e-3);
                let back = 1.0 + (3.0 * d / dist).min(0.25);
                pos = pos.map(|x| x * back);
                o.pos = pos;
                o.color = color;
                o.light = FULL_BRIGHT;
                // Unshaded, and its back faces dropped.
                o.normal = [0, 0, 0, 1];
                o.layer &= !0xFFFF;
                o.glint_uv = [0.0; 2];
                if !solid && let Some(g) = g {
                    o.layer |= 7;
                    o.glint_uv = g.uv([(pos[0] + pos[2]) * 8.0, pos[1] * 8.0]);
                }
                out.push(o);
            }
        }
    }
    if !cfg.outline_item_glint {
        // The mod's `removeRenderPass`: the shimmer is the outline's alone.
        for v in &mut verts[a..b] {
            if v.layer & 0xFFFF == 7 {
                v.layer &= !0xFFFF;
                v.glint_uv = [0.0; 2];
            }
        }
    }
    verts.extend(out);
}

/// Draws the held items at A&B's `rightitem` / `leftitem` bones, as
/// `ItemInHandLayer` does at the arm: the game draws no held items for a mob a
/// mod draws itself. `hands` are the engine's frames (see `Host::hands`).
fn draw_hand_items(
    game: &super::Game<'_>,
    draw: &mut super::BlockEntityDraw,
    hands: [Option<render::Mat4>; 2],
    items: [Option<&ItemStack>; 2],
    first_person: bool,
    seed: i32,
) -> [Option<(usize, usize)>; 2] {
    use horizon_assets::item_model::DisplayContext;
    let cfg = config();
    let mut spans = [None, None];
    for (i, (frame, stack)) in hands.iter().zip(items).enumerate() {
        let (Some(m), Some(stack)) = (frame, stack) else { continue };
        // The jar's `FirstPerson` holds items in A&B's first person hand with
        // the third person transforms too: the first person ones place an
        // item for vanilla's hand space and draw it huge at the hand.
        let ctx = if i == 0 { DisplayContext::ThirdPersonRightHand } else { DisplayContext::ThirdPersonLeftHand };
        let cols = m.0;
        let g = glam::Mat4::from_cols_array_2d(&[
            [cols[0][0], cols[1][0], cols[2][0], cols[3][0]],
            [cols[0][1], cols[1][1], cols[2][1], cols[3][1]],
            [cols[0][2], cols[1][2], cols[2][2], cols[3][2]],
            [cols[0][3], cols[1][3], cols[2][3], cols[3][3]],
        ]);
        let (scale, rot, pos) = g.to_scale_rotation_translation();
        if !pos.is_finite() || !rot.is_finite() {
            continue;
        }
        draw.push();
        draw.translate(pos.to_array());
        let (axis, angle) = rot.to_axis_angle();
        if angle.abs() > 1e-5 {
            draw.rotate_about(axis.to_array(), angle.to_degrees(), [0.0; 3]);
        }
        draw.scale(scale.to_array());
        if first_person {
            draw.scale([cfg.fp_item_scale; 3]);
        }
        // ItemInHandLayer: the item stands out of the fist, pointing ahead.
        draw.rotate_about([1.0, 0.0, 0.0], cfg.item_pitch, [0.0; 3]);
        draw.rotate_about([0.0, 1.0, 0.0], 180.0, [0.0; 3]);
        let side = if i == 0 { -1.0 } else { 1.0 };
        draw.translate([side * cfg.item_offset[0], cfg.item_offset[1], cfg.item_offset[2]]);
        let before = draw.vertices_mut().len();
        game.native_draw_item(draw, stack, ctx, None, seed + i as i32);
        let after = draw.vertices_mut().len();
        if after > before {
            spans[i] = Some((before, after));
        }
        draw.pop();
    }
    spans
}

/// A trace in the game log (latest.log) while the local player holds an item
/// named `ans_debug`: the walk and turn variables of A&B's scripts and what
/// the game answered, about twice a second.
fn trace(game: &super::Game<'_>, host: &Host, id: u32, who: &Who<'_>, view: &str) {
    let named = game.visible_hand_items()[0]
        .as_ref()
        .and_then(|s| s.data())
        .and_then(|d| d.custom_name_plain())
        .is_some_and(|n| n.trim_matches('"') == "ans_debug");
    if !named {
        return;
    }
    thread_local! { static N: std::cell::Cell<u32> = const { std::cell::Cell::new(0) }; }
    let n = N.with(|c| {
        c.set(c.get().wrapping_add(1));
        c.get()
    });
    if n % 30 != 0 {
        return;
    }
    let vars = host.var_trace(
        id,
        &["esnrkj", "lzwweo", "cknrcn", "mrmeib", "sfdeke", "nqexps", "eqhdel", "tjbdov", "nwocdu", "txtwqb", "iajodc", "qpjczw", "tnlxsa"],
    );
    let (pos, prev) = who.mob.map_or(([0.0; 3], [0.0; 3]), |m| {
        ([m.position.x, m.position.y, m.position.z], [m.previous.x, m.previous.y, m.previous.z])
    });
    // Sprites the game's atlas doesn't have draw nothing (`native_draw_quad`).
    let mut missing: Vec<&str> = host.quads.iter().map(|q| &*q.sprite).filter(|s| !game.gui.atlas.sprites.contains_key(*s)).collect();
    missing.sort_unstable();
    missing.dedup();
    let p = game.player.as_ref();
    log::info!(
        "ans_beyond trace {view}: ground={:?} sprint={:?} quads={} missing={missing:?} att=[{}]",
        p.map(|p| p.ground.on_ground),
        p.map(|p| p.sprinting),
        host.quads.len(),
        host.att_trace()
    );
    log::info!(
        "ans_beyond trace {view}: body={:.1} head={:.1} pos={:?} prev={:?} mob={} local={} | {vars}",
        who.body_yaw,
        who.head_yaw,
        pos,
        prev,
        who.mob.is_some(),
        who.hotbar.is_some()
    );
}

/// `native_player_submit`: a player with A&B's player model and its skin.
pub fn draw_player(
    game: &super::Game<'_>,
    render: &super::NativeMobRender<'_>,
    is_local: bool,
    draw: &mut super::BlockEntityDraw,
    drawn: &mut bool,
) {
    if *drawn || !game.native_on(ID) {
        return;
    }
    // Our own player in third person is the game's `player_model` (the game
    // says which player is this window's: `NativePlayerRender.local`; the
    // model's id is not the player's entity id).
    let own = game.player_model.as_ref().filter(|m| is_local || m.id == render.id);
    let local = own.is_some();
    let mob = own.or_else(|| mob_by_id(game, render.id));
    let skin = mob.and_then(|m| m.skin.clone()).unwrap_or_else(|| "entity/player/wide/steve".into());
    draw_with(game, render, mob, Some(&skin), local, draw, drawn);
}

/// `native_frame`: a new frame, and the sounds the animations fired.
pub fn frame(game: &mut super::Game<'_>) {
    menu_frame(game);
    let sounds = HOST.with(|h| {
        let mut host = h.borrow_mut();
        host.begin_frame();
        std::mem::take(&mut host.sounds)
    });
    if sounds.is_empty() || !game.native_on(ID) {
        return;
    }
    for s in sounds {
        let at = mob_by_id(game, s.mob).map(|m| [m.position.x, m.position.y, m.position.z]);
        if let Some(at) = at {
            game.native_play_local_sound(&s.event, at, 1.0, 1.0, 0);
        }
    }
}

/// `native_client_resources_reloaded`: packs changed, read the data again.
pub fn reload() {
    HOST.with(|h| {
        let mut host = h.borrow_mut();
        host.lib.clear();
        // A pack may bring textures of other sizes.
        host.sprite_sizes.clear();
    });
    SIZES_KNOWN.with(|k| k.set(false));
    MENU.with(|m| m.borrow_mut().rows = None);
}

/// Options > Mods: the settings page lists A&B's pack settings, each a
/// button that steps through its options.
pub fn settings_buttons(game: &super::Game<'_>, buttons: &mut Vec<String>) {
    let src = Resources(game.native_client_resources());
    HOST.with(|h| {
        let mut host = h.borrow_mut();
        host.load_settings(&src);
        load_settings(game, &mut host);
        for d in &host.settings.defs {
            let v = host.settings.value(&d.id).unwrap_or("").to_string();
            buttons.push(format!("{}: {}", d.readable.replace('_', " "), v));
        }
    });
}

pub fn settings_click(game: &super::Game<'_>, button: usize) {
    HOST.with(|h| {
        let mut host = h.borrow_mut();
        let Some(d) = host.settings.defs.get(button).cloned() else { return };
        let options: Vec<String> = if d.toggle { vec!["true".into(), "false".into()] } else { d.options.clone() };
        if options.is_empty() {
            return;
        }
        let cur = host.settings.value(&d.id).unwrap_or("").to_string();
        let next = options.iter().position(|o| *o == cur).map(|i| (i + 1) % options.len()).unwrap_or(0);
        host.settings.chosen.insert(d.id.clone(), options[next].clone());
        game.native_set_setting(ID, &setting_key(&d.id), &options[next]);
    });
}


/// The local player's queries in first person, from Horizon's movement.
struct LocalEnv<'a, 'g> {
    game: &'a super::Game<'g>,
    items: &'a [Option<horizon_client::items::ItemStack>; 2],
    walk: (f32, f32),
    who: Who<'a>,
}

impl Env for LocalEnv<'_, '_> {
    fn query(&mut self, _scope: &mut Scope, name: &str, args: &[Value]) -> Value {
        let num = |x: f64| Value::Num(x as f32);
        let p = self.game.player.as_ref();
        let item = |i: usize| self.items[i].as_ref().map(|s| s.item.to_string()).unwrap_or_default();
        if let Some(v) = shared_query(self.game, &self.who, name, args) {
            return v;
        }
        if let Some(v) = local_armor_query(self.game, name, args) {
            return v;
        }
        match name {
            "is_alive" => Value::bool(p.is_some_and(|p| !p.health.dead_or_dying())),
            "is_first_person" | "is_local_player" => Value::Num(1.0),
            "is_in_ui" | "is_attached" | "is_emoting" | "is_spectator" => Value::ZERO,
            "graphics_mode_is_any" => Value::bool(has_name(args, "fancy") || has_name(args, "deferred")),
            "is_owner_identifier_any" => Value::bool(has_name(args, "minecraft:player")),
            "modified_move_speed" | "walk_speed" => Value::Num(self.walk.1),
            "modified_distance_moved" | "walk_distance" => Value::Num(self.walk.0),
            "is_moving" => Value::bool(self.walk.1 > 0.01),
            "is_on_ground" => Value::bool(p.is_some_and(|p| p.ground.on_ground)),
            "is_sneaking" => Value::bool(p.is_some_and(|p| p.local_crouching)),
            "is_sprinting" => Value::bool(p.is_some_and(|p| p.sprinting)),
            "is_swimming" => Value::bool(p.is_some_and(|p| p.swimming)),
            "is_gliding" => Value::bool(p.is_some_and(|p| p.fall_flying)),
            "is_sleeping" => Value::bool(p.is_some_and(|p| p.sleeping)),
            "is_riding" => Value::bool(p.is_some_and(|p| p.is_passenger)),
            "is_jumping" => Value::bool(p.is_some_and(|p| p.jumping)),
            "is_on_fire" => Value::bool(p.is_some_and(|p| p.fire_ticks > 0)),
            "is_using_item" => Value::bool(self.game.used_hand().is_some()),
            "vertical_speed" => num(p.map_or(0.0, |p| p.velocity.y * 20.0)),
            "ground_speed" => num(p.map_or(0.0, |p| (p.velocity.x * p.velocity.x + p.velocity.z * p.velocity.z).sqrt() * 20.0)),
            "body_x_rotation" => num(f64::from(self.game.pitch)),
            "time_stamp" => num(self.game.world.game_time as f64),
            "model_scale" => Value::Num(1.0),
            "base_swing_duration" | "modified_swing_duration" => Value::Num(0.3),
            "health" => num(p.map_or(20.0, |p| f64::from(p.health.health()))),
            "max_health" => Value::Num(20.0),
            "has_head_gear" => Value::ZERO,
            "is_item_equipped" => Value::bool(!item(if args.first().map(Value::num).unwrap_or(0.0) >= 1.0 { 1 } else { 0 }).is_empty()),
            "get_equipped_item_name" => {
                let i = item(if args.first().map(Value::num).unwrap_or(0.0) >= 1.0 { 1 } else { 0 });
                Value::str(i.strip_prefix("minecraft:").unwrap_or(&i))
            }
            "is_item_name_any" => {
                let slot = match args.first().and_then(Value::as_str) {
                    Some("slot.weapon.mainhand") => 0,
                    Some("slot.weapon.offhand") => 1,
                    _ => return Value::ZERO,
                };
                let id = item(slot);
                Value::bool(args[1..].iter().filter_map(Value::as_str).any(|n| {
                    n == id || (id.is_empty() && (n.is_empty() || n == "minecraft:air"))
                }))
            }
            _ => Value::ZERO,
        }
    }
}

/// A held item's attachable that uses a texture the game's atlas lacks would
/// draw nothing (`native_draw_quad` skips it): drop it, so the item is drawn
/// as the game draws it, at A&B's hand.
fn drop_undrawable(game: &super::Game<'_>, host: &mut Host) {
    for slot in 0..2 {
        let (a, b) = host.att_quads[slot];
        // An empty sprite is a quad left out on purpose (a glow half with no
        // glow, a model the camera is inside).
        if host.quads[a..b].iter().any(|q| !q.sprite.is_empty() && !game.gui.atlas.sprites.contains_key(&*q.sprite)) {
            host.drop_attachable(slot);
        }
    }
}

/// The camera in the first person model's space: `first_person`'s
/// `to_world` without the sway (the eye, less `fp_arm_offset_*`, and the arm
/// scale about the shoulders undone).
fn fp_eye() -> [f32; 3] {
    let c = config();
    let o = c.fp_arm_offset;
    let eye = [-o[0], 1.62 - o[1], -o[2]];
    let k = c.fp_arm_scale.max(0.01);
    [eye[0] / k, 1.4 + (eye[1] - 1.4) / k, eye[2] / k]
}

/// A thrown item's swing brings it up to the face, and a model the camera is
/// inside (a pearl's or a bottle's glass shell) would cover the whole screen
/// for those frames: its quads are left out while the camera is in the box
/// they span. It still counts as drawn, so the game draws no flat item there.
fn hide_through_camera(host: &mut Host) {
    let eye = fp_eye();
    // The model's own boxes: a swimming, gliding or sprinting pose swings the
    // other arm (or the head) through the camera, where it fills the screen.
    // A box the camera is in or nearly touches is left out; the jar's first
    // person draws that arm from its own off-hand pose instead.
    const NEAR: f32 = 0.05;
    let spans = host.cube_spans.clone();
    for (a, b) in spans {
        if b <= a || b > host.quads.len() {
            continue;
        }
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for q in &host.quads[a..b] {
            for c in &q.corners {
                for i in 0..3 {
                    lo[i] = lo[i].min(c[i]);
                    hi[i] = hi[i].max(c[i]);
                }
            }
        }
        let d2: f32 = (0..3).map(|i| (lo[i] - eye[i]).max(0.0).max(eye[i] - hi[i]).powi(2)).sum();
        if d2 < NEAR * NEAR {
            for q in &mut host.quads[a..b] {
                q.sprite = std::rc::Rc::from("");
            }
        }
    }
    for slot in 0..2 {
        let (a, b) = host.att_quads[slot];
        if b <= a || b > host.quads.len() {
            continue;
        }
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for q in &host.quads[a..b] {
            for c in &q.corners {
                for i in 0..3 {
                    lo[i] = lo[i].min(c[i]);
                    hi[i] = hi[i].max(c[i]);
                }
            }
        }
        let margin = 0.05;
        if (0..3).all(|i| eye[i] > lo[i] - margin && eye[i] < hi[i] + margin) {
            for q in &mut host.quads[a..b] {
                q.sprite = std::rc::Rc::from("");
            }
        }
    }
}

/// A&B's per-player engine variables (`PlayerQueries.engineVariables`):
/// `v.is_holding_right` (main hand not empty), `v.is_holding_left`,
/// `v.player_x_rotation`, `v.gliding_speed_value` (`glidingSpeed`),
/// `v.is_holding_spyglass` (scoping), `v.goiapv` (main hand item has foil),
/// `v.is_tooting_goat_horn` and `v.is_using_brush`.
#[allow(clippy::too_many_arguments)]
fn player_vars(
    game: &super::Game<'_>,
    main: Option<&ItemStack>,
    off: Option<&ItemStack>,
    pitch: f32,
    fall_flying: bool,
    velocity: [f64; 3],
    using_ticks: i32,
) -> [(&'static str, f32); 8] {
    let b = |v: bool| if v { 1.0 } else { 0.0 };
    let glide = if fall_flying {
        let s = ((velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2]) as f32) / 0.2;
        (s * s * s).max(1.0)
    } else {
        1.0
    };
    let using = |name: &str| using_ticks > 0 && main.is_some_and(|s| s.item.ends_with(name));
    [
        ("is_holding_right", b(main.is_some())),
        ("is_holding_left", b(off.is_some())),
        ("player_x_rotation", pitch),
        ("gliding_speed_value", glide),
        ("is_holding_spyglass", b(using("spyglass"))),
        ("goiapv", b(main.is_some_and(|s| s.has_foil(game.res.items().enchantments.props.get(&*s.item).is_some_and(|p| p.glint))))),
        ("is_tooting_goat_horn", b(using("goat_horn"))),
        ("is_using_brush", b(using("brush"))),
    ]
}

/// Which hand each of `hand_geometry`'s runs draws, and whether it is an arm:
/// the main hand's empty arm, or an item (a filled map with the arms under
/// it), then the off hand's item. None when the count doesn't match (a hand
/// hidden or cancelled by a mod, the player invisible).
fn run_hands(n: usize, items: [Option<&ItemStack>; 2]) -> Option<Vec<(usize, bool)>> {
    let map = |s: &ItemStack| s.item.ends_with("filled_map");
    let mut full = Vec::new();
    let mut bare = Vec::new();
    for (hand, item) in items.into_iter().enumerate() {
        match item {
            None if hand == 0 => full.push((0, true)),
            None => {}
            Some(s) => {
                if map(s) {
                    full.push((hand, true));
                }
                full.push((hand, false));
                bare.push((hand, false));
            }
        }
    }
    if full.len() == n {
        Some(full)
    } else if bare.len() == n {
        Some(bare)
    } else {
        None
    }
}

/// Advances the local player's walk cycle to `ticks`, as vanilla's
/// `WalkAnimationState.update` does once a tick.
fn local_walk(game: &super::Game<'_>, ticks: f64) -> (f32, f32) {
    let moved = game.player.as_ref().map_or(0.0, |p| {
        (p.velocity.x * p.velocity.x + p.velocity.z * p.velocity.z).sqrt() as f32
    });
    WALK.with(|w| {
        let mut w = w.borrow_mut();
        let steps = (ticks - w.2).clamp(0.0, 5.0);
        if w.2 == 0.0 || ticks < w.2 {
            w.2 = ticks;
            return (w.0, w.1);
        }
        let target = (moved * 4.0).min(1.0);
        let k = 1.0 - (1.0 - 0.4f32).powf(steps as f32);
        w.1 += (target - w.1) * k;
        w.0 += w.1 * steps as f32;
        w.2 = ticks;
        (w.0, w.1)
    })
}

/// `hand_geometry`'s result (RETURN): in first person, A&B's own arms,
/// posed by its first-person animations, take the place of vanilla's arm.
/// Held items stay vanilla's for now. Built as A&B's `FirstPerson.draw`:
/// the player model drawn at the camera, 1.62 below the eye.
pub fn first_person(
    game: &mut super::Game<'_>,
    alpha: f64,
    value: (Vec<horizon_client::mesher::Vertex>, Vec<(u32, i32)>),
) -> (Vec<horizon_client::mesher::Vertex>, Vec<(u32, i32)>) {
    let (out, runs) = value;
    // Nothing drawn (F1, spectator, sleeping, dead) or turned off: as vanilla.
    if out.is_empty() || !game.native_on(ID) || game.player.is_none() {
        return (out, runs);
    }
    let game = &*game;
    let a = alpha as f32;
    let items = game.visible_hand_items();
    let ticks = game.world.game_time as f64 + f64::from(a);
    let walk = local_walk(game, ticks);
    let swing = game.swing.animation(a);
    let mut vars = vec![
        ("is_first_person", 1.0),
        ("is_paperdoll", 0.0),
        ("map_face_icon", 0.0),
        ("bob_animation", 1.0),
        ("attack_time", swing),
    ];
    {
        let p = game.player.as_ref();
        let using = game.player_model.as_ref().map_or(0, |m| m.extra.using_item);
        let velocity = p.map_or([0.0; 3], |p| [p.velocity.x, p.velocity.y, p.velocity.z]);
        vars.extend(player_vars(game, items[0].as_ref(), items[1].as_ref(), game.pitch, p.is_some_and(|p| p.fall_flying), velocity, using));
    }
    let heights = game.heights.interpolated(a);
    let context = [("player_arm_height", heights[0]), ("player_offhand_arm_height", heights[1])];
    let src = Resources(game.native_client_resources());
    let skin = game
        .player_model
        .as_ref()
        .and_then(|m| m.skin.clone())
        .unwrap_or_else(|| "entity/player/wide/steve".into());
    let mut quads = Vec::new();
    let mut fp_hands: [Option<render::Mat4>; 2] = [None, None];
    let mut fp_att = [(0usize, 0usize); 2];
    let mut fp_held_model = (0usize, 0usize);
    let mut attached = [false; 2];
    HOST.with(|h| {
        let mut host = h.borrow_mut();
        host.load_settings(&src);
        load_settings(game, &mut host);
        host.set_attachables(config().attachables);
        let held_ids = [0, 1].map(|i| items[i].as_ref().map(|s| s.item.to_string()).unwrap_or_default());
        let frame = MobFrame {
            id: LOCAL,
            kind: "minecraft:player",
            age_ticks: ticks,
            hurt: false,
            on_fire: false,
            skin: Some(&skin),
            vars: &vars,
            context: &context,
            held: [held_ids[0].as_str(), held_ids[1].as_str()],
            worn: [""; 4],
        };
        let eye = game.eye_position().unwrap_or_default();
        let pm = game.player_model.as_ref();
        let using = pm.map_or(0, |m| m.extra.using_item);
        let id = |i: usize| items[i].as_ref().map(|s| s.item.to_string()).unwrap_or_default();
        let who = Who {
            pos: [eye[0], eye[1] - 1.62, eye[2]],
            eye: 1.62,
            body_yaw: game.yaw,
            head_yaw: game.yaw,
            pitch: game.pitch,
            mob: game.player_model.as_ref(),
            hands: [id(0), id(1)],
            hotbar: Some(&game.inventory[..9.min(game.inventory.len())]),
            using_ticks: using,
            blocking: pm.is_some_and(|m| m.extra.blocking),
            charged: items[0].as_ref().and_then(|s| s.data()).is_some_and(|d| !d.charged.is_empty()),
        };
        SKIN_OK.with(|c| c.set(expression_skin(&skin)));
        composite_sizes(game, &mut host);
        host.change_color = 0;
        let mut env = LocalEnv { game, items: &items, walk, who };
        let drew = host.draw_mob(&src, &frame, &mut env);
        trace(game, &host, LOCAL, &env.who, "first person");
        drop_undrawable(game, &mut host);
        let k = config().fp_item_scale;
        host.scale_attachable(0, k);
        host.scale_attachable(1, k);
        hide_through_camera(&mut host);
        if drew {
            fp_att = [host.att_quads[0], host.att_quads[1]];
            quads = std::mem::take(&mut host.quads);
            fp_hands = host.hands;
            for (slot, h) in fp_hands.iter_mut().enumerate() {
                if host.drew_attachable(LOCAL, slot) {
                    *h = None;
                    attached[slot] = true;
                }
            }
            // The model's own held armour piece or elytra (Actions & Stuff's
            // 3D one): the game's item is not drawn too.
            fp_held_model = host.held_model;
            if fp_held_model.1 > fp_held_model.0 {
                fp_hands[0] = None;
                attached[0] = true;
            }
        }
    });
    if quads.is_empty() {
        return (out, runs);
    }
    let follow = config().held_items && config().first_person_items_follow_arm;
    // Vanilla's runs, hand by hand (`hand_geometry`'s order). Its arms go: A&B
    // draws its own. An item goes when its attachable drew it, or when it
    // follows A&B's hand (drawn below); otherwise it stays where vanilla holds
    // it, as the jar's `javaHands` draws it (`submitArmWithItem`).
    let owners = run_hands(runs.len(), [items[0].as_ref(), items[1].as_ref()]);
    let mut kept = Vec::with_capacity(out.len());
    let mut kept_runs = Vec::with_capacity(runs.len());
    for (i, &(start, id)) in runs.iter().enumerate() {
        let end = runs.get(i + 1).map_or(out.len(), |r| r.0 as usize);
        let drop = match &owners {
            Some(o) => {
                let (hand, arm) = o[i];
                arm || attached[hand] || (follow && fp_hands[hand].is_some())
            }
            // Not the runs expected (another mod changed the hands): only
            // what can be told apart, vanilla's arm (id 0 without a shader pack).
            None => id == 0 || attached.iter().any(|&a| a) || follow,
        };
        if drop {
            continue;
        }
        kept_runs.push((kept.len() as u32, id));
        let at = kept.len();
        kept.extend_from_slice(&out[start as usize..end]);
        // The game's own enchanted item, still drawn: its outline too.
        if let Some(o) = &owners
            && !o[i].1
            && foil(game, items[o[i].0].as_ref())
        {
            let n = kept.len();
            outline(game, &mut kept, at, n);
        }
    }
    // The eye's light, as vanilla lights the hands.
    let eye = game.eye_position().unwrap_or_default();
    let light = game.world.light(eye[0].floor() as i32, eye[1].floor() as i32, eye[2].floor() as i32);
    let packed = (u32::from(light >> 4) * 16) | ((u32::from(light & 15) * 16) << 8);
    let to_world = game.sway.to_world(a, &game.view)
        * glam::Mat4::from_translation({
            // `fp_arm_offset_*` in ans_beyond.txt: moves A&B's first person arms (view space, blocks).
            let o = config().fp_arm_offset;
            glam::Vec3::new(o[0], o[1] - 1.62, o[2])
        })
        // `fp_arm_scale`: about the shoulders (1.4 blocks up the model).
        * {
            let shoulders = glam::Vec3::new(0.0, 1.4, 0.0);
            glam::Mat4::from_translation(shoulders)
                * glam::Mat4::from_scale(glam::Vec3::splat(config().fp_arm_scale))
                * glam::Mat4::from_translation(-shoulders)
        };
    let mut draw = super::BlockEntityDraw::posed(
        a,
        game.world.game_time,
        packed,
        [0; 3],
        to_world,
        horizon_client::block_entity_render::diffuse_lights(false),
        Vec::new(),
    );
    let foils = [0, 1].map(|i| foil(game, items[i].as_ref()));
    let mut glint: Vec<(usize, (usize, usize))> = (0..2).filter(|&i| foils[i]).map(|i| (i, fp_att[i])).collect();
    if foils[0] && fp_held_model.1 > fp_held_model.0 {
        glint.push((0, fp_held_model));
    }
    let mut spans = draw_quads(game, &mut draw, &quads, packed, &glint);
    if follow {
        let drawn = draw_hand_items(game, &mut draw, fp_hands, [items[0].as_ref(), items[1].as_ref()], true, 0);
        for i in 0..2 {
            if spans[i].is_none() {
                spans[i] = drawn[i];
            }
        }
    }
    for i in 0..2 {
        if let (true, Some((a, b))) = (foils[i], spans[i]) {
            outline(game, draw.vertices_mut(), a, b);
        }
    }
    let mut arm = draw.into_vertices();
    for v in &mut arm {
        v.layer &= !horizon_client::gfx::OIT_ENTITY;
    }
    kept_runs.push((kept.len() as u32, 0));
    kept.extend(arm);
    (kept, kept_runs)
}

// ---------------------------------------------------------------------------
// Expression skins (the jar's `ExpressionSkins`): the pack's eyes and blinks
// read their pixels from parts of the skin a skin normally leaves empty. The
// game makes those parts opaque when it loads a skin (`setNoAlpha`), so on a
// skin not drawn for them the face would turn black: the jar turns the pack's
// expressions setting off for a skin whose original pixels there hold fewer
// than two opaque colours, or that is an old 64x32 one.

thread_local! {
    /// Whether the skin of the player being drawn is drawn for expressions.
    static SKIN_OK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static SKIN_KINDS: RefCell<std::collections::HashMap<String, bool>> = RefCell::new(std::collections::HashMap::new());
}

/// `ExpressionSkins.RECTS`: x, y, width, height.
const EXPRESSION_RECTS: [[u32; 4]; 11] = [
    [0, 0, 8, 8], [24, 0, 16, 8], [56, 0, 8, 8],
    [0, 16, 4, 4], [12, 16, 8, 4], [36, 16, 8, 4], [52, 16, 4, 4],
    [0, 32, 4, 4], [12, 32, 8, 4], [36, 32, 4, 2], [40, 32, 2, 1],
];

/// `ExpressionSkins.classify` over a skin's original RGBA.
fn expression_pixels(w: u32, h: u32, rgba: &[u8]) -> bool {
    if w != 64 || h != 64 {
        return false;
    }
    let mut colours = std::collections::HashSet::new();
    for [x0, y0, rw, rh] in EXPRESSION_RECTS {
        for y in y0..y0 + rh {
            for x in x0..x0 + rw {
                let i = ((y * w + x) * 4) as usize;
                if let Some(p) = rgba.get(i..i + 4) {
                    if p[3] >= 128 {
                        colours.insert([p[0], p[1], p[2]]);
                    }
                }
            }
        }
    }
    colours.len() >= 2
}

/// Whether `skin` (a texture location: `skins/<digest>` for a downloaded
/// one) is drawn for expressions, from its original file: the game's skin
/// cache (`assets/skins/<2>/<digest>`), or the offline skin.
fn expression_skin(skin: &str) -> bool {
    if let Some(v) = SKIN_KINDS.with(|k| k.borrow().get(skin).copied()) {
        return v;
    }
    let file = if let Some(d) = skin.strip_prefix("skins/") {
        let dir = std::env::current_dir().unwrap_or_default().join("assets/skins");
        Some(dir.join(d.get(..2).unwrap_or("xx")).join(d))
    } else if skin.starts_with("entity/player/") {
        // The default skins leave those parts empty.
        None
    } else {
        std::env::var_os("HORIZON_OFFLINE_SKIN").filter(|p| !p.is_empty()).map(std::path::PathBuf::from)
    };
    let ok = file
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|b| horizon_client::hud::decode_bytes(&b).ok())
        .is_some_and(|(w, h, rgba)| expression_pixels(w, h, &rgba));
    SKIN_KINDS.with(|k| k.borrow_mut().insert(skin.to_string(), ok));
    ok
}

// ---------------------------------------------------------------------------
// The Actions & Stuff screen: a key (`menu_key`, K) opens it over the world.
// It follows the jar's own settings hub (`AnsHubScreen`, 1.13 port3): a
// sidebar of categories (General, Visuals, Item outline, Animations, Sounds,
// and Viewmodel for this port's hand and item settings), the category's title
// picture, rows of an icon, a title and a description in the jar's Minecraft
// Ten font with a sliding switch or a value stepper, and a preview of the
// player on the right; the Nether colours of its menu theme and its interface
// sounds. The pictures, font and sounds come with the content mod (`ans_ui`).
// Each change is written to `config/ans_beyond.txt` at once.

// The jar's Nether menu theme.
const P_PANEL: u32 = 0xF61D_0709;
const P_SIDE: u32 = 0xFF14_0405;
const P_TITLEBAR: u32 = 0xFF17_0506;
const P_HOVER: u32 = 0xFF2D_150E;
const P_LINE: u32 = 0xFF3A_0E14;
const P_ACCENT: u32 = 0xFFE8_641C;
const P_ACCENT_HI: u32 = 0xFFFF_9A52;
const P_KNOB: u32 = 0xFFFF_FAF9;
const P_OFF_KNOB: u32 = 0xFFBE_B9B7;
const P_OFF_TRACK: u32 = 0xFF35_0F12;
const P_FRAME: u32 = 0xFF0E_0203;
const P_SCROLL: u32 = 0xFF5D_5A58;
const P_TRACK: u32 = 0xFF2A_0B0E;
const P_PREVIEW_TOP: u32 = 0xFF1A_0507;
const P_PREVIEW_BOTTOM: u32 = 0xFF3A_090D;
const WHITE: u32 = 0xFFFFFF;
const GREY: u32 = 0xB4B0AE;
const DARK: u32 = 0x1A0507;

/// `AnsHubScreen.ICON_NAMES`: the 16 pixel icons of `ans_ui:gui/icons`, in order.
const ICONS: [&str; 24] = [
    "sliders", "eye", "frame", "play", "speaker", "hand", "sparkle", "leaf", "person", "smile", "shirt", "shield",
    "pickaxe", "arrow", "drop", "chick", "fish", "totem", "palette", "star", "note", "run", "reset", "back",
];

/// The categories: the jar's (`AnsHubScreen.Cat`) and the port's own.
const CATS: [(&str, &str, &str); 6] = [
    ("general", "General", "sliders"),
    ("visuals", "Visuals", "eye"),
    ("outline", "Item outline", "sparkle"),
    ("animations", "Animations", "play"),
    ("sounds", "Sounds", "speaker"),
    ("viewmodel", "Viewmodel", "hand"),
];

/// One of the jar's pack settings as the screen shows it.
#[derive(Clone)]
struct PackRow {
    id: String,
    /// `readable` in `_settings.json` (`tools_3d`): picks its category and icon.
    readable: String,
    name: String,
    desc: String,
    kind: PackKind,
    options: Vec<(String, String)>,
}

#[derive(Clone, Copy, PartialEq)]
enum PackKind {
    Toggle,
    Dropdown,
    Multi,
}

/// What a click on a row does.
#[derive(Clone)]
enum Act {
    /// A number of ours: key, step, low, high (left click up, right down).
    Num(&'static str, f32, f32, f32),
    /// An on/off of ours.
    Flag(&'static str),
    /// A pack dropdown or toggle: next option (right click: the one before).
    Pack(usize),
    /// One option of a pack multiselect turned on or off.
    Multi(usize, usize),
    /// A category of the sidebar.
    Cat(usize),
    Reset,
    Done,
    None,
}

#[derive(Default)]
struct MenuState {
    scroll: f32,
    scroll_target: f32,
    close: bool,
    cat: usize,
    /// Last frame's buttons, for the clicks: `[x, y, w, h]`, what each does,
    /// and its stepper's box (a click on its left half steps back).
    hits: Vec<([f32; 4], Act, Option<[f32; 4]>)>,
    rows: Option<Vec<PackRow>>,
    max_scroll: f32,
    /// Each switch's knob, 0 off to 1 on, sliding towards its value.
    knobs: std::collections::HashMap<String, f32>,
    /// Interface sounds to play (`ans_ui:ui.*`), from the clicks.
    sounds: Vec<&'static str>,
    last: Option<std::time::Instant>,
    /// Whether the jar's Ten font is there (`ans_ui:ten`).
    ten: Option<bool>,
}

thread_local! {
    static MENU: RefCell<MenuState> = RefCell::new(MenuState::default());
}

/// `AnsHubScreen.catOf`: a pack setting's category, by its readable name.
fn cat_of(readable: &str) -> &'static str {
    if readable.starts_with("3d_arrows") {
        return "visuals";
    }
    match readable {
        "seasonal_mob_variants" => "general",
        "holstered_weapons" | "armor_visibility" | "tools_3d" | "axolotl_bucket_color" | "classic_baby_chicken" | "rain_particles"
        | "totem_translucency" | "biome_sky_colors" | "particles" => "visuals",
        "armour_shimmer" | "enchant_look" | "enchant_color" => "outline",
        "special_movement_animations" => "animations",
        "hotbar_sounds" => "sounds",
        _ => "general",
    }
}

/// `AnsHubScreen.iconOf`: a pack setting's icon.
fn icon_of(readable: &str) -> &'static str {
    if readable.starts_with("3d_arrows") {
        return "arrow";
    }
    match readable {
        "seasonal_mob_variants" | "biome_variants" => "leaf",
        "holstered_weapons" => "hand",
        "armour_shimmer" | "experience" | "particles" => "sparkle",
        "morph" => "person",
        "expressions" => "smile",
        "costumes" => "shirt",
        "armor_visibility" => "shield",
        "tools_3d" => "pickaxe",
        "axolotl_bucket_color" => "fish",
        "classic_baby_chicken" => "chick",
        "rain_particles" => "drop",
        "totem_translucency" => "totem",
        "enchant_look" => "star",
        "enchant_color" => "palette",
        "special_movement_animations" => "run",
        "hotbar_sounds" => "note",
        _ => "sparkle",
    }
}

/// Bedrock's `§` colour codes out (`§8`, `§r`).
fn plain(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '§' {
            it.next();
        } else {
            out.push(c);
        }
    }
    out
}

/// The jar's settings with their English names, read once.
fn pack_rows(game: &super::Game<'_>) -> Vec<PackRow> {
    if let Some(r) = MENU.with(|m| m.borrow().rows.clone()) {
        return r;
    }
    let src = Resources(game.native_client_resources());
    let read = |p: &str| src.read(p).and_then(|b| String::from_utf8(b).ok()).and_then(|t| ans_beyond_engine::json::parse(&t).ok());
    let lang = read("assets/ans/lang/en_us.json");
    let text = |k: &str| lang.as_ref().and_then(|l| l.get(k)).and_then(|v| v.as_str()).map(str::to_string);
    let mut rows = Vec::new();
    if let Some(defs) = read("assets/ans/entity/_settings.json") {
        for (id, v) in defs.entries() {
            let full = v.get("text").and_then(|t| t.as_str()).and_then(text).unwrap_or_default();
            let (name, desc) = match full.split_once("§8") {
                Some((n, d)) => (plain(n).trim().to_string(), plain(d).trim().to_string()),
                None => (plain(&full).trim().to_string(), String::new()),
            };
            // The Fabric port's own (`ans:`) settings are its Java code's, not the pack's.
            if id.starts_with("ans:") {
                continue;
            }
            let raw = v.get("readable").and_then(|t| t.as_str()).unwrap_or(id).to_string();
            let readable = raw.replace('_', " ");
            let kind = match v.get("type").and_then(|t| t.as_str()) {
                Some("toggle") => PackKind::Toggle,
                Some("multiselect") => PackKind::Multi,
                _ => PackKind::Dropdown,
            };
            let opts: Vec<&str> = v.get("options").map(|o| o.as_arr()).unwrap_or(&[]).iter().filter_map(|o| o.as_str()).collect();
            let texts: Vec<String> = v
                .get("option_texts")
                .map(|o| o.as_arr())
                .unwrap_or(&[])
                .iter()
                .filter_map(|o| o.as_str())
                .map(|k| plain(&text(k).unwrap_or_else(|| k.to_string())))
                .collect();
            let options = opts.iter().enumerate().map(|(i, o)| (o.to_string(), texts.get(i).cloned().unwrap_or_else(|| o.to_string()))).collect();
            rows.push(PackRow { id: id.clone(), readable: raw, name: if name.is_empty() { readable } else { name }, desc, kind, options });
        }
    }
    MENU.with(|m| m.borrow_mut().rows = Some(rows.clone()));
    rows
}

/// Writes `key = value` into `config/ans_beyond.txt` (replacing its line, or
/// added at the end) and reads the file again.
fn cfg_set(key: &str, value: &str) {
    let Some(path) = config_path() else { return };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut done = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| {
            let k = l.split('#').next().unwrap_or("").split('=').next().unwrap_or("").trim();
            if !done && k == key && l.contains('=') {
                done = true;
                format!("{key} = {value}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if !done {
        out.push(format!("{key} = {value}"));
    }
    let new = out.join("\n") + "\n";
    if std::fs::write(&path, &new).is_ok() {
        CFG.with(|c| {
            let mut c = c.borrow_mut();
            c.0 = parse_cfg(&new);
            c.1 = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        });
    }
}

fn pack_value(id: &str) -> String {
    HOST.with(|h| h.borrow().settings.value(id).unwrap_or("").to_string())
}

fn set_pack(id: &str, value: &str) {
    HOST.with(|h| {
        h.borrow_mut().settings.chosen.insert(id.to_string(), value.to_string());
    });
    cfg_set(id, value);
}

fn port_num(key: &str) -> f32 {
    let c = config();
    match key {
        "fp_arm_scale" => c.fp_arm_scale,
        "fp_item_scale" => c.fp_item_scale,
        "fp_arm_offset_x" => c.fp_arm_offset[0],
        "fp_arm_offset_y" => c.fp_arm_offset[1],
        "fp_arm_offset_z" => c.fp_arm_offset[2],
        "enchant_outline_color" => c.outline_color,
        "enchant_outline_width" => c.outline_width,
        "enchant_outline_style" => c.outline_style,
        _ => 0.0,
    }
}

fn port_flag(key: &str) -> bool {
    let c = config();
    match key {
        "held_items" => c.held_items,
        "attachables" => c.attachables,
        "armor" => c.armor,
        "first_person_items_follow_arm" => c.first_person_items_follow_arm,
        "enchant_outline" => c.outline,
        "enchant_outline_item_glint" => c.outline_item_glint,
        _ => false,
    }
}

/// `native_key`: the menu key opens the screen while playing, and closes it.
pub fn menu_key(game: &mut super::Game<'_>, key: &str, pressed: bool) {
    if !pressed || !game.native_on(ID) || key != config().menu_key {
        return;
    }
    if game.native_mod_screen() == Some(ID) {
        game.native_mod_screen_close(ID);
        ui_sound(game, "ans_ui:ui.close");
    } else if game.native_playing() {
        MENU.with(|m| {
            let mut m = m.borrow_mut();
            m.scroll = 0.0;
            m.scroll_target = 0.0;
            m.ten = None;
        });
        game.native_mod_screen_open(ID);
        ui_sound(game, "ans_ui:ui.page");
    }
}

/// From `frame`: a Done click closes the screen (the click hook can't).
fn menu_frame(game: &mut super::Game<'_>) {
    let (close, sounds) = MENU.with(|m| {
        let mut m = m.borrow_mut();
        (std::mem::take(&mut m.close), std::mem::take(&mut m.sounds))
    });
    for s in sounds {
        ui_sound(game, s);
    }
    if close {
        game.native_mod_screen_close(ID);
        ui_sound(game, "ans_ui:ui.close");
    }
}

/// One of the jar's interface sounds, heard where the player is.
fn ui_sound(game: &mut super::Game<'_>, event: &str) {
    if let Some(at) = game.eye_position() {
        game.native_play_local_sound(event, at, 0.5, 1.0, 0);
    }
}

/// The skin's front, flat (as the skin screen's preview): head and hat,
/// body and jacket, arms and sleeves, legs and trousers. `px` GUI pixels a
/// skin pixel; `at` the head's top left.
fn draw_skin(game: &super::Game<'_>, b: &mut horizon_client::hud::HudBatch, skin: &str, at: [f32; 2], px: f32) {
    let slim = skin.contains("/slim/");
    let aw = if slim { 3.0 } else { 4.0 };
    let [x, y] = at;
    let mut part = |src: [f32; 4], dx: f32, dy: f32| {
        let dst = [x + dx * px, y + dy * px, src[2] * px, src[3] * px];
        game.native_sprite(b, skin, src, dst);
    };
    // Base layer, then the overlay one.
    for over in [false, true] {
        let o = |base: [f32; 2], top: [f32; 2]| if over { top } else { base };
        let head = o([8.0, 8.0], [40.0, 8.0]);
        part([head[0], head[1], 8.0, 8.0], aw, 0.0);
        let body = o([20.0, 20.0], [20.0, 36.0]);
        part([body[0], body[1], 8.0, 12.0], aw, 8.0);
        // The player's right arm is on the viewer's left.
        let ra = o([44.0, 20.0], [44.0, 36.0]);
        part([ra[0], ra[1], aw, 12.0], 0.0, 8.0);
        let la = o([36.0, 52.0], [52.0, 52.0]);
        part([la[0], la[1], aw, 12.0], aw + 8.0, 8.0);
        let rl = o([4.0, 20.0], [4.0, 36.0]);
        part([rl[0], rl[1], 4.0, 12.0], aw, 20.0);
        let ll = o([20.0, 52.0], [4.0, 52.0]);
        part([ll[0], ll[1], 4.0, 12.0], aw + 4.0, 20.0);
    }
}

/// One row of the settings list.
struct UiRow {
    icon: &'static str,
    title: String,
    desc: String,
    ctl: Ctl,
    act: Act,
    /// An option of a multiselect, under its setting.
    indent: bool,
}

/// A row's control: a sliding switch, a value stepper, or none (a group).
enum Ctl {
    Switch(String, bool),
    Value(String),
    None,
}

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |s: u32| {
        let x = ((a >> s) & 0xFF) as f32;
        let y = ((b >> s) & 0xFF) as f32;
        ((x + (y - x) * t).round() as u32) << s
    };
    ch(24) | ch(16) | ch(8) | ch(0)
}

/// The jar's Ten font when the content mod brought it, else the game's.
fn ten_style() -> horizon_client::text::Style {
    let ten = MENU.with(|m| m.borrow().ten).unwrap_or(false);
    horizon_client::text::Style { font: ten.then(|| "ans_ui:ten".to_string()), ..Default::default() }
}

fn ui_text_width(game: &super::Game<'_>, s: &str) -> f32 {
    let t = horizon_client::text::Text::styled(s, ten_style());
    game.gui.atlas.font.width_text(&t)
}

fn ui_text(game: &super::Game<'_>, b: &mut horizon_client::hud::HudBatch, s: &str, at: [f32; 2], rgb: u32, shadow: bool) {
    let t = horizon_client::text::Text::styled(s, ten_style());
    let [_, r, g, bl] = rgb.to_be_bytes();
    b.text_styled(&game.gui.atlas, &t, at[0], at[1], horizon_client::hud::rgba(r, g, bl, 0xFF), shadow);
}

/// `s` cut to `w` pixels, with an ellipsis when cut.
fn fit(game: &super::Game<'_>, s: &str, w: f32) -> String {
    if ui_text_width(game, s) <= w {
        return s.to_string();
    }
    let mut t: String = s.to_string();
    while !t.is_empty() && ui_text_width(game, &format!("{t}...")) > w {
        t.pop();
    }
    format!("{}...", t.trim_end())
}

/// `s` in lines of at most `w` pixels (at most `max`; the last one cut).
fn wrap(game: &super::Game<'_>, s: &str, w: f32, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in s.split_whitespace() {
        let t = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if ui_text_width(game, &t) > w && !line.is_empty() {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        } else {
            line = t;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.len() > max {
        let rest = lines[max - 1..].join(" ");
        lines.truncate(max - 1);
        lines.push(fit(game, &rest, w));
    }
    lines
}

fn icon(game: &super::Game<'_>, b: &mut horizon_client::hud::HudBatch, name: &str, at: [f32; 2], size: f32) {
    if let Some(i) = ICONS.iter().position(|n| *n == name) {
        game.native_sprite(b, "ans_ui:gui/icons", [i as f32 * 16.0, 0.0, 16.0, 16.0], [at[0], at[1], size, size]);
    }
}

fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}

/// The rows of category `cat`.
fn cat_rows(rows: &[PackRow], cat: usize) -> Vec<UiRow> {
    let on = |v: bool| v;
    let mut out = Vec::new();
    if CATS[cat].0 == "viewmodel" {
        let num = |k: &'static str, icon: &'static str, title: &str, desc: &str, step: f32, lo: f32, hi: f32, fmt: &dyn Fn(f32) -> String| UiRow {
            icon,
            title: title.to_string(),
            desc: desc.to_string(),
            ctl: Ctl::Value(fmt(port_num(k))),
            act: Act::Num(k, step, lo, hi),
            indent: false,
        };
        let flag = |k: &'static str, icon: &'static str, title: &str, desc: &str| UiRow {
            icon,
            title: title.to_string(),
            desc: desc.to_string(),
            ctl: Ctl::Switch(k.to_string(), on(port_flag(k))),
            act: Act::Flag(k),
            indent: false,
        };
        let x = |v: f32| format!("{:.2}x", v);
        let sign = |v: f32| format!("{:+.2}", v);
        out.push(num("fp_arm_scale", "hand", "Arm size", "First person arm size, about the shoulders.", 0.05, 0.3, 2.0, &x));
        out.push(num("fp_item_scale", "pickaxe", "Item size", "First person size of 3D held items, about the hand holding them.", 0.05, 0.3, 2.0, &x));
        out.push(num("fp_arm_offset_y", "sliders", "Arm height", "Moves the first person arms up or down.", 0.02, -1.0, 1.0, &sign));
        out.push(num("fp_arm_offset_x", "sliders", "Arm side", "Moves the first person arms right or left.", 0.02, -1.0, 1.0, &sign));
        out.push(num("fp_arm_offset_z", "sliders", "Arm depth", "Moves the first person arms back or forward.", 0.02, -1.0, 1.0, &sign));
        out.push(flag("attachables", "pickaxe", "3D held items", "Held items as the pack's 3D models, with their own hand animations."));
        out.push(flag("held_items", "hand", "Held items", "Draw held items in the model's hands."));
        out.push(flag("first_person_items_follow_arm", "hand", "Items follow arm", "An item without a 3D model is drawn in the animated hand."));
        out.push(flag("armor", "shield", "Worn armour", "Draw worn armour on the model."));
        return out;
    }
    if CATS[cat].0 == "outline" {
        let row = |icon: &'static str, title: &str, desc: &str, ctl: Ctl, act: Act| UiRow { icon, title: title.into(), desc: desc.into(), ctl, act, indent: false };
        out.push(row("sparkle", "Enchantment outline", "An outline around enchanted items held in hand (Enchantment Glint Outline).", Ctl::Switch("enchant_outline".into(), port_flag("enchant_outline")), Act::Flag("enchant_outline")));
        let style = if port_num("enchant_outline_style") >= 0.5 { "Solid" } else { "Glint" };
        out.push(row("star", "Outline style", "Glint: the enchantment shimmer on the outline. Solid: one colour.", Ctl::Value(style.into()), Act::Num("enchant_outline_style", 1.0, 0.0, 1.0)));
        let c = port_num("enchant_outline_color").round() as usize;
        out.push(row("palette", "Outline colour", "The solid outline's colour; rainbow cycles through them.", Ctl::Value(OUTLINE_COLORS.get(c).map_or("Lilac", |o| o.0).to_string()), Act::Num("enchant_outline_color", 1.0, 0.0, (OUTLINE_COLORS.len() - 1) as f32)));
        out.push(row("sliders", "Outline size", "How thick the outline is.", Ctl::Value(format!("{:.0}", port_num("enchant_outline_width") * 1000.0)), Act::Num("enchant_outline_width", 0.002, 0.005, 0.1)));
        out.push(row("sparkle", "Shimmer on the item", "The item keeps its own glint under the outline.", Ctl::Switch("enchant_outline_item_glint".into(), port_flag("enchant_outline_item_glint")), Act::Flag("enchant_outline_item_glint")));
    }
    for (i, r) in rows.iter().enumerate() {
        if cat_of(&r.readable) != CATS[cat].0 {
            continue;
        }
        let v = pack_value(&r.id);
        let icon = icon_of(&r.readable);
        match r.kind {
            PackKind::Toggle => out.push(UiRow { icon, title: r.name.clone(), desc: r.desc.clone(), ctl: Ctl::Switch(r.id.clone(), v == "true"), act: Act::Pack(i), indent: false }),
            PackKind::Dropdown => {
                let label = r.options.iter().find(|o| o.0 == v).map(|o| o.1.clone()).unwrap_or(v);
                let label = label.split(':').next().unwrap_or("").trim().to_string();
                out.push(UiRow { icon, title: r.name.clone(), desc: r.desc.clone(), ctl: Ctl::Value(label), act: Act::Pack(i), indent: false });
            }
            PackKind::Multi => {
                out.push(UiRow { icon, title: r.name.clone(), desc: r.desc.clone(), ctl: Ctl::None, act: Act::None, indent: false });
                let chosen: Vec<&str> = v.split(',').map(str::trim).collect();
                for (j, (o, label)) in r.options.iter().enumerate() {
                    let (t, d) = match label.split_once(':') {
                        Some((t, d)) => (t.trim().to_string(), d.trim().to_string()),
                        None => (label.clone(), String::new()),
                    };
                    out.push(UiRow { icon, title: t, desc: d, ctl: Ctl::Switch(format!("{}#{o}", r.id), chosen.contains(&o.as_str())), act: Act::Multi(i, j), indent: true });
                }
            }
        }
    }
    out
}

/// `native_screen_render_post`: draws the screen when it is ours.
pub fn menu_draw(game: &super::Game<'_>, b: &mut horizon_client::hud::HudBatch, screen: &horizon_client::mod_api::NativeScreenView<'_>) {
    if game.native_mod_screen() != Some(ID) {
        return;
    }
    // Is the Ten font there? A font the game doesn't have draws as boxes.
    if MENU.with(|m| m.borrow().ten.is_none()) {
        let f = &game.gui.atlas.font;
        let ten = format!("{:?}", f.glyph_in("ans_ui:ten", 'A')) != format!("{:?}", f.glyph_in("ans_ui:no_such_font", 'A'));
        MENU.with(|m| m.borrow_mut().ten = Some(ten));
    }
    let dt = MENU.with(|m| {
        let mut m = m.borrow_mut();
        let now = std::time::Instant::now();
        let dt = m.last.map_or(0.0, |l| now.duration_since(l).as_secs_f32()).min(0.1);
        m.last = Some(now);
        dt
    });
    let rows = pack_rows(game);
    let cat = MENU.with(|m| m.borrow().cat).min(CATS.len() - 1);
    let [_, _, sw, sh] = screen.rect;
    let mouse = screen.mouse;
    let w = (sw - 16.0).clamp(360.0, 640.0).min(sw);
    let h = (sh - 16.0).clamp(220.0, 380.0).min(sh);
    let (x0, y0) = (((sw - w) / 2.0).floor(), ((sh - h) / 2.0).floor());
    let mut hits: Vec<([f32; 4], Act, Option<[f32; 4]>)> = Vec::new();
    game.native_fill(b, [0.0, 0.0, sw, sh], 0x8C00_0000);
    game.native_fill(b, [x0 - 1.0, y0 - 1.0, w + 2.0, h + 2.0], P_FRAME);
    game.native_fill(b, [x0, y0, w, h], P_PANEL);

    // The sidebar: one icon a category.
    let side = 28.0;
    game.native_fill(b, [x0, y0, side, h], P_SIDE);
    game.native_fill(b, [x0 + side, y0, 1.0, h], P_LINE);
    let mut tip = None;
    for (i, (_, name, ic)) in CATS.iter().enumerate() {
        let r = [x0 + 2.0, y0 + 6.0 + i as f32 * 26.0, side - 4.0, 22.0];
        let over = inside(r, mouse);
        if i == cat {
            game.native_fill(b, r, P_HOVER);
            game.native_fill(b, [x0, r[1], 2.0, r[3]], P_ACCENT);
        } else if over {
            game.native_fill(b, r, 0xFF22_0A0C);
        }
        icon(game, b, ic, [r[0] + (r[2] - 16.0) / 2.0, r[1] + 3.0], 16.0);
        if over {
            tip = Some((name.to_string(), [r[0] + r[2] + 6.0, r[1] + 7.0]));
        }
        hits.push((r, Act::Cat(i), None));
    }

    // The preview on the right.
    let pw = (w * 0.30).clamp(110.0, 190.0).floor();
    let px0 = x0 + w - pw;
    let bands = 12;
    for i in 0..bands {
        let t = i as f32 / (bands - 1) as f32;
        let bh = h / bands as f32;
        game.native_fill(b, [px0, y0 + i as f32 * bh, pw, bh + 1.0], mix(P_PREVIEW_TOP, P_PREVIEW_BOTTOM, t));
    }
    game.native_fill(b, [px0 - 1.0, y0, 1.0, h], P_LINE);
    // A few specks, as the jar's backdrop.
    let mut seed = 0x9E37_79B9u32;
    for _ in 0..28 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let sx = px0 + 4.0 + (seed % 1000) as f32 / 1000.0 * (pw - 8.0);
        let sy = y0 + 4.0 + ((seed / 1000) % 1000) as f32 / 1000.0 * (h - 8.0);
        game.native_fill(b, [sx.floor(), sy.floor(), 1.0, 1.0], if seed % 3 == 0 { 0x80FF_FFFF } else { 0x40FF_FFFF });
    }
    let skin = game.player_model.as_ref().and_then(|m| m.skin.clone()).unwrap_or_else(|| "entity/player/wide/steve".into());
    let px = ((pw - 30.0) / 16.0).min((h - 96.0) / 32.0).floor().clamp(2.0, 7.0);
    let skin_y = y0 + 16.0;
    draw_skin(game, b, &skin, [px0 + (pw - 16.0 * px) / 2.0, skin_y], px);
    let name = game.own_profile_name().to_uppercase();
    let ny = skin_y + 32.0 * px + 8.0;
    ui_text(game, b, &name, [px0 + (pw - ui_text_width(game, &name)) / 2.0, ny], WHITE, true);
    if let Some(r) = rows.iter().find(|r| r.readable == "morph") {
        let v = pack_value(&r.id);
        let label = r.options.iter().find(|o| o.0 == v).map(|o| o.1.clone()).unwrap_or(v);
        let label = label.split(':').next().unwrap_or("").trim().to_uppercase();
        ui_text(game, b, &label, [px0 + (pw - ui_text_width(game, &label)) / 2.0, ny + 11.0], P_ACCENT & 0xFF_FFFF, false);
    }
    // Reset and Done under it.
    let bw = ((pw - 18.0) / 2.0).floor();
    for (i, (label, act)) in [("RESET", Act::Reset), ("DONE", Act::Done)].into_iter().enumerate() {
        let r = [px0 + 6.0 + i as f32 * (bw + 6.0), y0 + h - 24.0, bw, 18.0];
        let over = inside(r, mouse);
        let done = matches!(act, Act::Done);
        game.native_fill(b, [r[0] - 1.0, r[1] - 1.0, r[2] + 2.0, r[3] + 2.0], P_FRAME);
        let fill = if done { if over { P_ACCENT_HI } else { P_ACCENT } } else if over { P_HOVER } else { P_OFF_TRACK };
        game.native_fill(b, r, fill);
        game.native_fill(b, [r[0], r[1] + r[3] - 2.0, r[2], 2.0], mix(fill, 0xFF00_0000, 0.3));
        let col = if done { DARK } else { WHITE };
        ui_text(game, b, label, [r[0] + (r[2] - ui_text_width(game, label)) / 2.0, r[1] + 5.0], col, false);
        hits.push((r, act, None));
    }

    // The title picture of the category.
    let lx = x0 + side + 1.0;
    let lw = px0 - 1.0 - lx;
    game.native_fill(b, [lx, y0, lw, 30.0], P_TITLEBAR);
    game.native_fill(b, [lx, y0 + 30.0, lw, 1.0], P_LINE);
    let title_sprite = format!("ans_ui:gui/titles/{}", CATS[cat].0);
    match game.native_sprite_size(&title_sprite) {
        Some([tw, th]) => {
            let th2 = 20.0;
            let tw2 = (tw * th2 / th).min(lw - 20.0);
            game.native_sprite(b, &title_sprite, [0.0, 0.0, tw, th], [lx + 10.0, y0 + 5.0, tw2, th2]);
        }
        None => ui_text(game, b, &CATS[cat].1.to_uppercase(), [lx + 10.0, y0 + 11.0], P_ACCENT & 0xFF_FFFF, true),
    }

    // The rows.
    let list = cat_rows(&rows, cat);
    let (top, bottom) = (y0 + 31.0, y0 + h - 2.0);
    let ctl_w = 76.0;
    let text_x = |indent: bool| lx + if indent { 40.0 } else { 30.0 };
    let text_w = |indent: bool| lw - (text_x(indent) - lx) - ctl_w - 22.0;
    let laid: Vec<(Vec<String>, f32)> = list
        .iter()
        .map(|r| {
            let lines = wrap(game, &r.desc.to_uppercase(), text_w(r.indent), 2);
            let hgt = 14.0 + 9.0 * lines.len().max(1) as f32 + 6.0;
            (lines, hgt.max(28.0))
        })
        .collect();
    let total: f32 = laid.iter().map(|l| l.1).sum::<f32>() + 8.0;
    let max_scroll = (total - (bottom - top)).max(0.0);
    let scroll = MENU.with(|m| {
        let mut m = m.borrow_mut();
        m.max_scroll = max_scroll;
        m.scroll_target = m.scroll_target.clamp(0.0, max_scroll);
        let k = 1.0 - (-dt * 18.0).exp();
        m.scroll += (m.scroll_target - m.scroll) * k;
        if (m.scroll - m.scroll_target).abs() < 0.3 {
            m.scroll = m.scroll_target;
        }
        m.scroll
    });
    let mut y = top + 4.0 - scroll;
    let mut switches: Vec<(String, bool)> = Vec::new();
    for (r, (lines, hgt)) in list.iter().zip(&laid) {
        let rect = [lx, y, lw - 6.0, *hgt];
        if y >= top - 0.5 && y + hgt <= bottom + 0.5 {
            let over = inside(rect, mouse) && !matches!(r.act, Act::None);
            if over {
                game.native_fill(b, rect, P_HOVER);
            }
            let ix = lx + if r.indent { 22.0 } else { 9.0 };
            icon(game, b, r.icon, [ix, y + (hgt - 16.0) / 2.0], 16.0);
            let tx = text_x(r.indent);
            let title = fit(game, &r.title.to_uppercase(), text_w(r.indent));
            ui_text(game, b, &title, [tx, y + 6.0], WHITE, true);
            for (i, l) in lines.iter().enumerate() {
                ui_text(game, b, l, [tx, y + 16.0 + i as f32 * 9.0], GREY, false);
            }
            let cx = lx + lw - 6.0 - ctl_w - 8.0;
            let cy = y + (hgt - 14.0) / 2.0;
            match &r.ctl {
                Ctl::Switch(key, on) => {
                    let sw_w = 34.0;
                    let sx = cx + ctl_w - sw_w;
                    let t = MENU.with(|m| {
                        let mut m = m.borrow_mut();
                        let k = m.knobs.entry(key.clone()).or_insert(if *on { 1.0 } else { 0.0 });
                        let goal = if *on { 1.0 } else { 0.0 };
                        *k += (goal - *k) * (1.0 - (-dt * 22.0).exp());
                        *k
                    });
                    switches.push((key.clone(), *on));
                    game.native_fill(b, [sx, cy, sw_w, 14.0], P_FRAME);
                    game.native_fill(b, [sx + 2.0, cy + 2.0, sw_w - 4.0, 10.0], mix(P_OFF_TRACK, P_ACCENT, t));
                    let kw = 13.0;
                    let kx = sx + 2.0 + t * (sw_w - 4.0 - kw);
                    game.native_fill(b, [kx.round(), cy + 2.0, kw, 10.0], mix(P_OFF_KNOB, P_KNOB, t));
                }
                Ctl::Value(v) => {
                    let bx = [cx, cy, ctl_w, 14.0];
                    game.native_fill(b, bx, P_FRAME);
                    game.native_fill(b, [cx + 1.0, cy + 1.0, ctl_w - 2.0, 12.0], P_OFF_TRACK);
                    let hover_left = over && mouse[0] < cx + ctl_w / 2.0 && inside(bx, mouse);
                    let hover_right = over && mouse[0] >= cx + ctl_w / 2.0 && inside(bx, mouse);
                    ui_text(game, b, "<", [cx + 4.0, cy + 3.0], if hover_left { 0xFFFFFF } else { P_ACCENT & 0xFF_FFFF }, false);
                    ui_text(game, b, ">", [cx + ctl_w - 4.0 - ui_text_width(game, ">"), cy + 3.0], if hover_right { 0xFFFFFF } else { P_ACCENT & 0xFF_FFFF }, false);
                    let v = fit(game, &v.to_uppercase(), ctl_w - 22.0);
                    ui_text(game, b, &v, [cx + (ctl_w - ui_text_width(game, &v)) / 2.0, cy + 3.0], WHITE, false);
                }
                Ctl::None => {}
            }
            let ctl = matches!(r.ctl, Ctl::Value(_)).then_some([cx, cy, ctl_w, 14.0]);
            if !matches!(r.act, Act::None) {
                hits.push((rect, r.act.clone(), ctl));
            }
        }
        y += hgt;
        if y > top && y < bottom {
            game.native_fill(b, [lx + 8.0, y - 1.0, lw - 22.0, 1.0], P_LINE);
        }
    }
    // Forget the knobs of switches no longer shown.
    MENU.with(|m| {
        let mut m = m.borrow_mut();
        m.knobs.retain(|k, _| switches.iter().any(|s| &s.0 == k));
    });
    // The scroll bar.
    if max_scroll > 0.0 {
        let track = bottom - top - 4.0;
        let bar = (track * track / (track + max_scroll)).max(14.0);
        let by = top + 2.0 + (track - bar) * scroll / max_scroll;
        game.native_fill(b, [px0 - 6.0, top + 2.0, 3.0, track], P_TRACK);
        game.native_fill(b, [px0 - 6.0, by, 3.0, bar], P_SCROLL);
    }
    // The category's name beside the sidebar, under the mouse.
    if let Some((t, at)) = tip {
        let t = t.to_uppercase();
        let tw = ui_text_width(game, &t);
        game.native_fill(b, [at[0] - 4.0, at[1] - 4.0, tw + 8.0, 15.0], P_FRAME);
        game.native_fill(b, [at[0] - 3.0, at[1] - 3.0, tw + 6.0, 13.0], P_TITLEBAR);
        ui_text(game, b, &t, at, WHITE, false);
    }
    MENU.with(|m| m.borrow_mut().hits = hits);
}

/// `native_screen_mouse_button_pressed_pre` on our screen: a button.
pub fn menu_click(game: &super::Game<'_>, button: i32, mouse: [f32; 2], canceled: &mut bool) {
    if game.native_mod_screen() != Some(ID) {
        return;
    }
    *canceled = true;
    let hit = MENU.with(|m| m.borrow().hits.iter().find(|(r, ..)| inside(*r, mouse)).map(|h| (h.1.clone(), h.2)));
    let Some((act, ctl)) = hit else { return };
    // Right click, or a click on a stepper's left half, steps back.
    let back = button == 1 || ctl.is_some_and(|c| inside(c, mouse) && mouse[0] < c[0] + c[2] / 2.0);
    let rows = pack_rows(game);
    let sound = |s: &'static str| MENU.with(|m| m.borrow_mut().sounds.push(s));
    let toggled = |on: bool| sound(if on { "ans_ui:ui.toggle_on" } else { "ans_ui:ui.toggle_off" });
    match act {
        Act::Num(k, step, lo, hi) => {
            let v = (port_num(k) + if back { -step } else { step }).clamp(lo, hi);
            cfg_set(k, &format!("{:.3}", v));
            sound("ans_ui:ui.tick");
        }
        Act::Flag(k) => {
            let on = !port_flag(k);
            cfg_set(k, if on { "true" } else { "false" });
            toggled(on);
        }
        Act::Pack(i) => {
            let r = &rows[i];
            let opts: Vec<String> = if r.kind == PackKind::Toggle { vec!["true".into(), "false".into()] } else { r.options.iter().map(|o| o.0.clone()).collect() };
            if opts.is_empty() {
                return;
            }
            let cur = pack_value(&r.id);
            let at = opts.iter().position(|o| *o == cur).unwrap_or(0);
            let next = if back { (at + opts.len() - 1) % opts.len() } else { (at + 1) % opts.len() };
            set_pack(&r.id, &opts[next]);
            if r.kind == PackKind::Toggle {
                toggled(opts[next] == "true");
            } else {
                sound("ans_ui:ui.tick");
            }
        }
        Act::Multi(i, j) => {
            let r = &rows[i];
            let cur = pack_value(&r.id);
            let mut on: Vec<String> = cur.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            let o = r.options[j].0.clone();
            let now_on = if let Some(p) = on.iter().position(|x| *x == o) {
                on.remove(p);
                false
            } else {
                on.push(o);
                true
            };
            // In the pack's own order.
            let order: Vec<String> = r.options.iter().map(|o| o.0.clone()).filter(|o| on.contains(o)).collect();
            set_pack(&r.id, &order.join(","));
            toggled(now_on);
        }
        Act::Cat(i) => {
            MENU.with(|m| {
                let mut m = m.borrow_mut();
                if m.cat != i {
                    m.cat = i;
                    m.scroll = 0.0;
                    m.scroll_target = 0.0;
                    m.sounds.push("ans_ui:ui.page");
                }
            });
        }
        Act::Reset => {
            for r in &rows {
                let d = HOST.with(|h| h.borrow().settings.defs.iter().find(|d| d.id == r.id).map(|d| d.default.clone()));
                if let Some(d) = d {
                    set_pack(&r.id, &d);
                }
            }
            let c = Cfg::default();
            cfg_set("fp_arm_scale", &c.fp_arm_scale.to_string());
            cfg_set("fp_item_scale", &c.fp_item_scale.to_string());
            for (i, k) in ["fp_arm_offset_x", "fp_arm_offset_y", "fp_arm_offset_z"].iter().enumerate() {
                cfg_set(k, &c.fp_arm_offset[i].to_string());
            }
            for k in ["held_items", "attachables", "armor"] {
                cfg_set(k, "true");
            }
            cfg_set("first_person_items_follow_arm", "false");
            sound("ans_ui:ui.tap");
        }
        Act::Done => MENU.with(|m| m.borrow_mut().close = true),
        Act::None => {}
    }
}

/// `native_screen_mouse_scrolled_pre` on our screen: the settings scroll.
pub fn menu_scroll(game: &super::Game<'_>, scroll: [f64; 2], canceled: &mut bool) {
    if game.native_mod_screen() != Some(ID) {
        return;
    }
    *canceled = true;
    MENU.with(|m| {
        let mut m = m.borrow_mut();
        m.scroll_target = (m.scroll_target - scroll[1] as f32 * 28.0).clamp(0.0, m.max_scroll);
    });
}
