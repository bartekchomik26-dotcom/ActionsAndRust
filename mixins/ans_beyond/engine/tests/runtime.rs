// The animation runtime against the behaviour reverse engineered from
// `EntityRuntime$AnimationPlayer.tick`, `$ControllerPlayer.tick` and
// `EntityBundle$Blend.outgoing`.
use ans_beyond_engine::{anim, bundle::Bundle, json, molang::{Env, Scope, Value}, runtime::Runtime};
use std::rc::Rc;

struct Q { moving: f32 }
impl Env for Q {
    fn query(&mut self, _: &mut Scope, n: &str, _: &[Value]) -> Value {
        match n { "is_moving" => Value::Num(self.moving), _ => Value::ZERO }
    }
}

fn setup(entity: &str, anims: &str) -> Runtime {
    let b = Bundle::parse(&json::parse(entity).unwrap());
    b.set_animations(anim::parse_file(&json::parse(anims).unwrap()));
    Runtime::new(Rc::new(b), 1)
}

const ANIMS: &str = r#"{"animations":{
  "a.walk":{"loop":true,"animation_length":1,"bones":{"leg":{"rotation":{"0":[0,0,0],"1":[40,0,0]}}}},
  "a.idle":{"loop":true,"bones":{"leg":{"rotation":[10,0,0]}}},
  "a.once":{"animation_length":0.5,"bones":{"arm":{"position":{"0":[0,0,0],"0.5":[0,2,0]}}}},
  "a.hold":{"loop":"hold_on_last_frame","animation_length":0.5,"bones":{"arm":{"position":{"0":[0,0,0],"0.5":[0,2,0]}}}}
}}"#;

#[test]
fn looping_animation_wraps_and_weights() {
    let mut rt = setup(r#"{"animations":{"walk":{"animation":"a.walk","loop":true}},
        "scripts":{"animate":[{"walk":"0.5"}]}}"#, ANIMS);
    let mut q = Q { moving: 1.0 };
    for _ in 0..30 { rt.tick(1.0 / 60.0, 0.0, &mut q); }
    // half a second in, weight 0.5: 20 deg * 0.5
    let r = rt.pose.get("leg")[0];
    assert!((r - 10.0).abs() < 0.2, "{r}");
    for _ in 0..60 { rt.tick(1.0 / 60.0, 0.0, &mut q); }
    let r = rt.pose.get("leg")[0];
    assert!((r - 10.0).abs() < 0.2, "wrapped: {r}");
}

#[test]
fn once_stops_contributing_hold_keeps_last_frame() {
    let mut once = setup(r#"{"animations":{"o":{"animation":"a.once"}},"scripts":{"animate":["o"]}}"#, ANIMS);
    let mut hold = setup(r#"{"animations":{"h":{"animation":"a.hold"}},"scripts":{"animate":["h"]}}"#, ANIMS);
    let mut q = Q { moving: 0.0 };
    for _ in 0..60 { once.tick(1.0 / 60.0, 0.0, &mut q); hold.tick(1.0 / 60.0, 0.0, &mut q); }
    assert_eq!(once.pose.get("arm")[4], 0.0);
    assert_eq!(hold.pose.get("arm")[4], 2.0);
}

#[test]
fn controller_transitions_and_cross_fades() {
    let mut rt = setup(r#"{"animations":{
        "walk":{"animation":"a.walk","loop":true},"idle":{"animation":"a.idle","loop":true},
        "ctl":{"controller":{"initial_state":"default","states":{
            "default":{"animations":["idle"],"transitions":[{"moving":"q.is_moving"}],"blend_transition":0.5,"on_exit":["v.left=1;"]},
            "moving":{"animations":["walk"],"transitions":[{"default":"!q.is_moving"}],"on_entry":["v.entered=v.entered+1;"]}}}}},
        "scripts":{"animate":["ctl"]}}"#, ANIMS);
    let mut q = Q { moving: 0.0 };
    for _ in 0..10 { rt.tick(1.0 / 60.0, 0.0, &mut q); }
    assert_eq!(rt.pose.get("leg")[0], 10.0);
    q.moving = 1.0;
    rt.tick(1.0 / 60.0, 0.0, &mut q);
    assert_eq!(rt.scope.var("left"), Some(&Value::Num(1.0)));
    assert_eq!(rt.scope.var("entered"), Some(&Value::Num(1.0)));
    // 0.25 s into a 0.5 s fade: idle at weight 0.5 still adds 5 degrees
    for _ in 0..14 { rt.tick(1.0 / 60.0, 0.0, &mut q); }
    let walk = 40.0 * 15.0 / 60.0; // walk's own time, 0.25 s
    let r = rt.pose.get("leg")[0];
    assert!((r - (10.0 * 0.5 + walk * 0.5)).abs() < 1.0, "{r}");
    for _ in 0..60 { rt.tick(1.0 / 60.0, 0.0, &mut q); }
    assert!(rt.pose.get("leg")[0] < 40.5);
}

#[test]
fn timeline_fires_once_per_pass() {
    let mut rt = setup(r#"{"animations":{"w":{"animation":"a.walk","loop":true,"timeline":{"0.5":"v.steps=(v.steps??0)+1;"}}},
        "scripts":{"animate":["w"]}}"#, ANIMS);
    let mut q = Q { moving: 1.0 };
    for _ in 0..160 { rt.tick(1.0 / 60.0, 0.0, &mut q); }
    // 2.67 s of a 1 s loop: passed 0.5 three times
    assert_eq!(rt.scope.var("steps"), Some(&Value::Num(3.0)));
}
