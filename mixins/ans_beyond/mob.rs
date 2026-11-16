// Draws a mob with its Actions & Beyond Bedrock model.
fn ans_beyond_mob(
    game: &crate::game::Game<'_>,
    mob: &crate::game::NativeMobRender<'_>,
    draw: &mut crate::game::BlockEntityDraw,
    drawn: &mut bool,
) {
    crate::game::ans_beyond::draw_mob(game, mob, draw, drawn);
}
