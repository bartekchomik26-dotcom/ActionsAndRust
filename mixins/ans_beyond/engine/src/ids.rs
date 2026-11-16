// Java entity type ids to the Bedrock names the entity bundles use
// (`assets/ans/entity/<name>.json`); the rest share their name.

pub fn bundle_name(java: &str) -> &str {
    let id = java.strip_prefix("minecraft:").unwrap_or(java);
    match id {
        "zombified_piglin" => "zombie_pigman",
        "villager" => "villager_v2",
        "zombie_villager" => "zombie_villager_v2",
        "evoker" => "evocation_illager",
        "evoker_fangs" => "evocation_fang",
        "end_crystal" => "ender_crystal",
        "experience_orb" => "xp_orb",
        "experience_bottle" => "xp_bottle",
        "tropical_fish" => "tropicalfish",
        "eye_of_ender" => "eye_of_ender_signal",
        "firework_rocket" => "fireworks_rocket",
        "trident" => "thrown_trident",
        "wind_charge" | "breeze_wind_charge" => "wind_charge_projectile",
        "fishing_bobber" => "fishing_hook",
        "mooshroom" => "mooshroom",
        "snow_golem" => "snow_golem",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn renames() {
        assert_eq!(super::bundle_name("minecraft:zombified_piglin"), "zombie_pigman");
        assert_eq!(super::bundle_name("minecraft:zombie"), "zombie");
    }
}
