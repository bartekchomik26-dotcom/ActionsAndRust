// The mouse wheel on the Actions & Stuff screen.
fn ans_beyond_menu_scroll(game: &crate::game::Game<'_>, scroll: [f64; 2], canceled: &mut bool) {
    crate::game::ans_beyond::menu_scroll(game, scroll, canceled);
}
