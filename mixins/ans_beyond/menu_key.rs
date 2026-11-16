// The menu key (`menu_key` in ans_beyond.txt, K) opens the Actions & Stuff screen.
fn ans_beyond_menu_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    crate::game::ans_beyond::menu_key(game, key, pressed);
}
