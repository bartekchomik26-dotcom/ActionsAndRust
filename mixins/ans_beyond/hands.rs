// First person: Actions & Beyond's arms replace vanilla's in the hand pass.
// `Vertex` is the type `hand_geometry`'s file imports: the RETURN value's
// type has to be spelled as the target function spells it.
fn ans_beyond_hands(
    game: &mut crate::game::Game<'_>,
    alpha: f64,
    value: (Vec<Vertex>, Vec<(u32, i32)>),
) -> (Vec<Vertex>, Vec<(u32, i32)>) {
    crate::game::ans_beyond::first_person(game, alpha, value)
}
