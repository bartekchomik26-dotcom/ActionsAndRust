// Draws a player with Actions & Beyond's player model and its own skin.
fn ans_beyond_player(
    game: &crate::game::Game<'_>,
    player: &crate::game::NativeMobRender<'_>,
    who: &crate::game::NativePlayerRender<'_>,
    drawn: &mut bool,
    draw: &mut crate::game::BlockEntityDraw,
) {
    crate::game::ans_beyond::draw_player(game, player, who.local, draw, drawn);
}
