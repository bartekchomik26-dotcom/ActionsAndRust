// Draws the Actions & Stuff screen when it is open.
fn ans_beyond_menu_draw(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    screen: &horizon_client::mod_api::NativeScreenView<'_>,
) {
    crate::game::ans_beyond::menu_draw(game, batch, screen);
}
