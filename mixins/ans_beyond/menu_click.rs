// A click on the Actions & Stuff screen.
fn ans_beyond_menu_click(
    game: &crate::game::Game<'_>,
    screen: &horizon_client::mod_api::NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    crate::game::ans_beyond::menu_click(game, button, screen.mouse, canceled);
}
