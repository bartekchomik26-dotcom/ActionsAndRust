// Once a frame: forgets mobs out of view and plays the animations' sounds.
fn ans_beyond_frame(game: &mut crate::game::Game<'_>) {
    crate::game::ans_beyond::frame(game);
}
