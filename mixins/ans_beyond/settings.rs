// The settings page: Actions & Beyond's pack settings, one button each.
fn ans_beyond_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id != crate::game::ans_beyond::ID {
        return;
    }
    crate::game::ans_beyond::settings_buttons(&*game, buttons);
}
