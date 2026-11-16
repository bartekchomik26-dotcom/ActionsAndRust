// A pack setting's button steps to its next option.
fn ans_beyond_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    if mod_id != crate::game::ans_beyond::ID {
        return;
    }
    crate::game::ans_beyond::settings_click(&*game, button);
}
