// The mod's row in Options > Mods.
fn ans_beyond_listed(mods: &mut Vec<horizon_client::mod_api::NativeMod>) {
    use crate::game::ans_beyond as m;
    mods.push(horizon_client::mod_api::NativeMod {
        id: m::ID.into(),
        title: format!("{} {}", m::NAME, m::VERSION),
        description: m::DESCRIPTION.into(),
        keys: Vec::new(),
    });
}
