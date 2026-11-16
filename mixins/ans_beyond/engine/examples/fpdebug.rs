use ans_beyond_engine::assets::{Dir, Library};
use ans_beyond_engine::molang::{self, Env, Scope, Value};
use ans_beyond_engine::runtime::Runtime;
struct Mock;
impl Env for Mock {
    fn query(&mut self, _: &mut Scope, name: &str, args: &[Value]) -> Value {
        Value::Num(match name { "is_alive" | "is_on_ground" => 1.0,
            "graphics_mode_is_any" => args.iter().any(|a| a.as_str() == Some("fancy")) as i32 as f32, "is_item_name_any" => args.iter().skip(1).any(|a| a.as_str() == Some("")) as i32 as f32, _ => 0.0 })
    }
}
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = Dir(a[1].clone().into());
    let mut lib = Library::default();
    let b = lib.bundle(&src, "minecraft:player").unwrap();
    let mut rt = Runtime::new(b.clone(), 1);
    for _ in 0..30 { rt.scope.set_var("is_first_person", Value::Num(1.0)); rt.tick(1.0/60.0, 5.0, &mut Mock); }
    for rc in &b.render {
        let c = rc.condition.as_ref().map(|c| rt.scope.run(c, &mut Mock)).unwrap_or(Value::Num(1.0));
        let g = rt.scope.run(&rc.geometry, &mut Mock);
        eprintln!("{} cond={:?} geo={:?}", rc.id, c, g);
    }
    for v in a[2..].iter() { eprintln!("v.{v} = {:?}", rt.scope.var(v)); }
    let e = molang::compile("v.tnlxsa").unwrap(); eprintln!("{:?}", rt.scope.run(&e, &mut Mock));
}
