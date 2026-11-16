// Molang: the expression language Bedrock entity, animation and render
// controller files are written in. This is a full reimplementation of what
// Actions & Beyond's `com.ans.client.molang` package evaluates: numbers and
// strings, v./t./c. variables (with dotted struct names), queries with
// arguments, Math.*, `? :`, binary `?`, `??`, `->`, `{}` blocks, `return`,
// `loop`, `for_each`, `break`, `continue`, `Geometry.`/`Texture.`/
// `Material.` references and `Array.x[i]`.

use std::cell::RefCell;

use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f32),
    Str(Rc<str>),
    /// Another entity, for `->` (a query such as `q.get_ride` returns one).
    Actor(u32),
    /// A struct of x, y, z (`q.bone_origin`): assigned to `v.a`, it is read
    /// back as `v.a.x`, `v.a.y`, `v.a.z`.
    Vec3([f32; 3]),
    /// `q.bone_orientation_trs`: translation, rotation (degrees) and scale,
    /// read back as `v.a.t.x`, `v.a.r.y`, `v.a.s.z`.
    Trs([f32; 9]),
}

impl Value {
    pub const ZERO: Value = Value::Num(0.0);
    pub fn num(&self) -> f32 {
        match self {
            Value::Num(n) => *n,
            Value::Str(_) => 0.0,
            Value::Actor(_) => 1.0,
            Value::Vec3(v) => v[0],
            Value::Trs(v) => v[0],
        }
    }
    pub fn truthy(&self) -> bool {
        match self {
            Value::Num(n) => *n != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Actor(_) | Value::Vec3(_) | Value::Trs(_) => true,
        }
    }
    pub fn str(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }
    pub fn bool(b: bool) -> Value {
        Value::Num(if b { 1.0 } else { 0.0 })
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self { Some(s) } else { None }
    }
}

/// Variable names, interned so scopes index by number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name(pub u32);

thread_local! {
    static NAMES: RefCell<(crate::FastMap<String, u32>, Vec<Rc<str>>)> = RefCell::new((crate::FastMap::default(), Vec::new()));
}

pub fn intern(s: &str) -> Name {
    NAMES.with(|n| {
        let mut n = n.borrow_mut();
        if let Some(&i) = n.0.get(s) {
            return Name(i);
        }
        let i = n.1.len() as u32;
        n.1.push(Rc::from(s));
        n.0.insert(s.to_string(), i);
        Name(i)
    })
}

pub fn name_str(n: Name) -> Rc<str> {
    NAMES.with(|t| t.borrow().1[n.0 as usize].clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarKind {
    Variable,
    Temp,
    Context,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f32),
    Str(Rc<str>),
    Var(VarKind, Name),
    /// `q.name(args)`; the name is lowercase without the `q.` prefix.
    Query(Rc<str>, Vec<Expr>),
    Math(MathFn, Vec<Expr>),
    /// `Geometry.x`, `Texture.x`, `Material.x`: "geometry.x" and so on.
    Ref(Rc<str>),
    /// `Array.name[i]`.
    Index(Rc<str>, Box<Expr>),
    This,
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Coalesce(Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `a ? b` with no `:`: b when a, else 0.
    If(Box<Expr>, Box<Expr>),
    Assign(VarKind, Name, Box<Expr>),
    Arrow(Box<Expr>, Box<Expr>),
    Block(Vec<Expr>),
    Return(Box<Expr>),
    Loop(Box<Expr>, Box<Expr>),
    ForEach(VarKind, Name, Box<Expr>, Box<Expr>),
    Break,
    Continue,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MathFn {
    Abs,
    Acos,
    Asin,
    Atan,
    Atan2,
    Ceil,
    Clamp,
    Cos,
    CopySign,
    DieRoll,
    DieRollInteger,
    Exp,
    Floor,
    HermiteBlend,
    InverseLerp,
    Lerp,
    LerpRotate,
    Ln,
    Max,
    Min,
    MinAngle,
    Mod,
    Pi,
    Pow,
    Random,
    RandomInteger,
    Round,
    Sign,
    Sin,
    Sqrt,
    Trunc,
    Ease(Ease),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InSine,
    OutSine,
    InOutSine,
    InExpo,
    OutExpo,
    InOutExpo,
    InCirc,
    OutCirc,
    InOutCirc,
    InBack,
    OutBack,
    InOutBack,
    InElastic,
    OutElastic,
    InOutElastic,
    InBounce,
    OutBounce,
    InOutBounce,
}

fn math_fn(name: &str) -> Option<MathFn> {
    use self::Ease::*;
    use MathFn::*;
    Some(match name {
        "abs" => Abs,
        "acos" => Acos,
        "asin" => Asin,
        "atan" => Atan,
        "atan2" => Atan2,
        "ceil" => Ceil,
        "clamp" => Clamp,
        "cos" => Cos,
        "copy_sign" => CopySign,
        "die_roll" => DieRoll,
        "die_roll_integer" => DieRollInteger,
        "exp" => Exp,
        "floor" => Floor,
        "hermite_blend" => HermiteBlend,
        "inverse_lerp" => InverseLerp,
        "lerp" => Lerp,
        "lerprotate" => LerpRotate,
        "ln" => Ln,
        "max" => Max,
        "min" => Min,
        "min_angle" => MinAngle,
        "mod" => Mod,
        "pi" => Pi,
        "pow" => Pow,
        "random" => Random,
        "random_integer" => RandomInteger,
        "round" => Round,
        "sign" => Sign,
        "sin" => Sin,
        "sqrt" => Sqrt,
        "trunc" => Trunc,
        "ease_in_quad" => MathFn::Ease(InQuad),
        "ease_out_quad" => MathFn::Ease(OutQuad),
        "ease_in_out_quad" => MathFn::Ease(InOutQuad),
        "ease_in_cubic" => MathFn::Ease(InCubic),
        "ease_out_cubic" => MathFn::Ease(OutCubic),
        "ease_in_out_cubic" => MathFn::Ease(InOutCubic),
        "ease_in_quart" => MathFn::Ease(InQuart),
        "ease_out_quart" => MathFn::Ease(OutQuart),
        "ease_in_out_quart" => MathFn::Ease(InOutQuart),
        "ease_in_quint" => MathFn::Ease(InQuint),
        "ease_out_quint" => MathFn::Ease(OutQuint),
        "ease_in_out_quint" => MathFn::Ease(InOutQuint),
        "ease_in_sine" => MathFn::Ease(InSine),
        "ease_out_sine" => MathFn::Ease(OutSine),
        "ease_in_out_sine" => MathFn::Ease(InOutSine),
        "ease_in_expo" => MathFn::Ease(InExpo),
        "ease_out_expo" => MathFn::Ease(OutExpo),
        "ease_in_out_expo" => MathFn::Ease(InOutExpo),
        "ease_in_circ" => MathFn::Ease(InCirc),
        "ease_out_circ" => MathFn::Ease(OutCirc),
        "ease_in_out_circ" => MathFn::Ease(InOutCirc),
        "ease_in_back" => MathFn::Ease(InBack),
        "ease_out_back" => MathFn::Ease(OutBack),
        "ease_in_out_back" => MathFn::Ease(InOutBack),
        "ease_in_elastic" => MathFn::Ease(InElastic),
        "ease_out_elastic" => MathFn::Ease(OutElastic),
        "ease_in_out_elastic" => MathFn::Ease(InOutElastic),
        "ease_in_bounce" => MathFn::Ease(InBounce),
        "ease_out_bounce" => MathFn::Ease(OutBounce),
        "ease_in_out_bounce" => MathFn::Ease(InOutBounce),
        _ => return None,
    })
}

// ---------------------------------------------------------------- lexer

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f32),
    Str(Rc<str>),
    Ident(String),
    Sym(&'static str),
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    const SYMS: [&str; 26] = [
        "->", "??", "==", "!=", "<=", ">=", "&&", "||", "<", ">", "+", "-", "*", "/", "!", "?", ":",
        "(", ")", "{", "}", "[", "]", ",", ";", "=",
    ];
    'outer: while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let s = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            // Exponents and the `f` suffix some packs write (`0.5f`).
            if i < b.len() && (b[i] == b'e' || b[i] == b'E')
                && b.get(i + 1).is_some_and(|d| d.is_ascii_digit() || *d == b'-' || *d == b'+')
            {
                i += 2;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let n: f32 = src[s..i].parse().map_err(|_| format!("bad number {}", &src[s..i]))?;
            if i < b.len() && (b[i] == b'f' || b[i] == b'F') {
                i += 1;
            }
            out.push(Tok::Num(n));
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                // A dot followed by something not a name ends the identifier.
                if b[i] == b'.' && !b.get(i + 1).is_some_and(|d| d.is_ascii_alphanumeric() || *d == b'_') {
                    break;
                }
                i += 1;
            }
            out.push(Tok::Ident(src[s..i].to_ascii_lowercase()));
            continue;
        }
        if c == b'\'' {
            let s = i + 1;
            i += 1;
            while i < b.len() && b[i] != b'\'' {
                i += 1;
            }
            out.push(Tok::Str(Rc::from(&src[s..i.min(b.len())])));
            i += 1;
            continue;
        }
        for s in SYMS {
            if src[i..].starts_with(s) {
                out.push(Tok::Sym(s));
                i += s.len();
                continue 'outer;
            }
        }
        return Err(format!("unexpected '{}' in {src}", c as char));
    }
    Ok(out)
}

// ---------------------------------------------------------------- parser

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

fn split_var(name: &str) -> Option<(VarKind, &str)> {
    let (p, rest) = name.split_once('.')?;
    let k = match p {
        "v" | "variable" => VarKind::Variable,
        "t" | "temp" => VarKind::Temp,
        "c" | "context" => VarKind::Context,
        _ => return None,
    };
    Some((k, rest))
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn is(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Tok::Sym(x)) if *x == s)
    }
    fn eat(&mut self, s: &str) -> bool {
        if self.is(s) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, s: &str) -> Result<(), String> {
        if self.eat(s) { Ok(()) } else { Err(format!("expected '{s}', got {:?}", self.peek())) }
    }

    /// statements separated by `;`, up to `}` or the end.
    fn statements(&mut self) -> Result<Vec<Expr>, String> {
        let mut out = Vec::new();
        loop {
            while self.eat(";") {}
            if self.peek().is_none() || self.is("}") {
                return Ok(out);
            }
            out.push(self.statement()?);
            if !self.eat(";") && !(self.peek().is_none() || self.is("}")) {
                return Err(format!("expected ';', got {:?}", self.peek()));
            }
        }
    }
    fn statement(&mut self) -> Result<Expr, String> {
        if let Some(Tok::Ident(w)) = self.peek() {
            match w.as_str() {
                "return" => {
                    self.i += 1;
                    return Ok(Expr::Return(Box::new(self.expr()?)));
                }
                "break" => {
                    self.i += 1;
                    return Ok(Expr::Break);
                }
                "continue" => {
                    self.i += 1;
                    return Ok(Expr::Continue);
                }
                _ => {}
            }
        }
        self.expr()
    }
    fn expr(&mut self) -> Result<Expr, String> {
        let lhs = self.ternary()?;
        if self.eat("=") {
            let rhs = self.expr()?;
            return match lhs {
                Expr::Var(k, n) => Ok(Expr::Assign(k, n, Box::new(rhs))),
                other => Err(format!("cannot assign to {other:?}")),
            };
        }
        Ok(lhs)
    }
    fn ternary(&mut self) -> Result<Expr, String> {
        let c = self.coalesce()?;
        if self.eat("?") {
            let a = self.ternary_arm()?;
            if self.eat(":") {
                let b = self.ternary_arm()?;
                return Ok(Expr::Ternary(Box::new(c), Box::new(a), Box::new(b)));
            }
            return Ok(Expr::If(Box::new(c), Box::new(a)));
        }
        Ok(c)
    }
    /// An arm of `? :` may itself be an assignment (`a ? v.x = 1 : ...`).
    fn ternary_arm(&mut self) -> Result<Expr, String> {
        let e = self.ternary()?;
        if self.is("=") {
            if let Expr::Var(k, n) = e {
                self.i += 1;
                let rhs = self.ternary()?;
                return Ok(Expr::Assign(k, n, Box::new(rhs)));
            }
        }
        Ok(e)
    }
    fn coalesce(&mut self) -> Result<Expr, String> {
        let mut l = self.or()?;
        while self.eat("??") {
            let r = self.or()?;
            l = Expr::Coalesce(Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn or(&mut self) -> Result<Expr, String> {
        let mut l = self.and()?;
        while self.eat("||") {
            let r = self.and()?;
            l = Expr::Or(Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn and(&mut self) -> Result<Expr, String> {
        let mut l = self.equality()?;
        while self.eat("&&") {
            let r = self.equality()?;
            l = Expr::And(Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn equality(&mut self) -> Result<Expr, String> {
        let mut l = self.relational()?;
        loop {
            let op = if self.eat("==") { Op::Eq } else if self.eat("!=") { Op::Ne } else { return Ok(l) };
            let r = self.relational()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }
    fn relational(&mut self) -> Result<Expr, String> {
        let mut l = self.additive()?;
        loop {
            let op = if self.eat("<=") {
                Op::Le
            } else if self.eat(">=") {
                Op::Ge
            } else if self.eat("<") {
                Op::Lt
            } else if self.eat(">") {
                Op::Gt
            } else {
                return Ok(l);
            };
            let r = self.additive()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }
    fn additive(&mut self) -> Result<Expr, String> {
        let mut l = self.multiplicative()?;
        loop {
            let op = if self.eat("+") { Op::Add } else if self.eat("-") { Op::Sub } else { return Ok(l) };
            let r = self.multiplicative()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }
    fn multiplicative(&mut self) -> Result<Expr, String> {
        let mut l = self.unary()?;
        loop {
            let op = if self.eat("*") { Op::Mul } else if self.eat("/") { Op::Div } else { return Ok(l) };
            let r = self.unary()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }
    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat("-") {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.eat("!") {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        if self.eat("+") {
            return self.unary();
        }
        let mut e = self.primary()?;
        while self.eat("->") {
            let r = self.primary()?;
            e = Expr::Arrow(Box::new(e), Box::new(r));
        }
        Ok(e)
    }
    fn args(&mut self) -> Result<Vec<Expr>, String> {
        let mut a = Vec::new();
        if self.eat("(") {
            if self.eat(")") {
                return Ok(a);
            }
            loop {
                a.push(self.expr()?);
                if self.eat(")") {
                    return Ok(a);
                }
                self.expect(",")?;
            }
        }
        Ok(a)
    }
    fn primary(&mut self) -> Result<Expr, String> {
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.i += 1;
                Ok(Expr::Num(n))
            }
            Some(Tok::Str(s)) => {
                self.i += 1;
                Ok(Expr::Str(s))
            }
            Some(Tok::Sym("(")) => {
                self.i += 1;
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Some(Tok::Sym("{")) => {
                self.i += 1;
                let s = self.statements()?;
                self.expect("}")?;
                Ok(Expr::Block(s))
            }
            Some(Tok::Ident(id)) => {
                self.i += 1;
                self.ident(&id)
            }
            other => Err(format!("unexpected {other:?}")),
        }
    }
    fn ident(&mut self, id: &str) -> Result<Expr, String> {
        match id {
            "break" => return Ok(Expr::Break),
            "continue" => return Ok(Expr::Continue),
            "return" => return Ok(Expr::Return(Box::new(self.expr()?))),
            "true" => return Ok(Expr::Num(1.0)),
            "false" => return Ok(Expr::Num(0.0)),
            "this" => return Ok(Expr::This),
            "loop" => {
                self.expect("(")?;
                let n = self.expr()?;
                self.expect(",")?;
                let body = self.expr()?;
                self.expect(")")?;
                return Ok(Expr::Loop(Box::new(n), Box::new(body)));
            }
            "for_each" => {
                self.expect("(")?;
                let var = self.expr()?;
                self.expect(",")?;
                let coll = self.expr()?;
                self.expect(",")?;
                let body = self.expr()?;
                self.expect(")")?;
                let Expr::Var(k, n) = var else { return Err("for_each needs a variable".into()) };
                return Ok(Expr::ForEach(k, n, Box::new(coll), Box::new(body)));
            }
            _ => {}
        }
        if let Some((k, rest)) = split_var(id) {
            return Ok(Expr::Var(k, intern(rest)));
        }
        let (p, rest) = id.split_once('.').unwrap_or((id, ""));
        match p {
            "q" | "query" => Ok(Expr::Query(Rc::from(rest), self.args()?)),
            "math" => {
                let f = math_fn(rest).ok_or_else(|| format!("unknown math.{rest}"))?;
                Ok(Expr::Math(f, self.args()?))
            }
            "geometry" | "texture" | "material" => Ok(Expr::Ref(Rc::from(format!("{p}.{rest}")))),
            "array" => {
                let name: Rc<str> = Rc::from(format!("array.{rest}"));
                if self.eat("[") {
                    let i = self.expr()?;
                    self.expect("]")?;
                    Ok(Expr::Index(name, Box::new(i)))
                } else {
                    Ok(Expr::Index(name, Box::new(Expr::Num(0.0))))
                }
            }
            // Bare names used as strings by some packs (render controller
            // part names, `default` and the like) read as 0 in Bedrock.
            _ => Ok(Expr::Num(0.0)),
        }
    }
}

/// A compiled Molang script. A script with statements (`;`) yields its
/// `return` value, else 0; a lone expression yields its value.
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    pub body: Vec<Expr>,
    pub complex: bool,
}

impl Script {
    pub fn constant(v: f32) -> Script {
        Script { body: vec![Expr::Num(v)], complex: false }
    }
    pub fn as_const(&self) -> Option<f32> {
        match self.body.as_slice() {
            [Expr::Num(n)] if !self.complex => Some(*n),
            _ => None,
        }
    }
}

pub fn compile(src: &str) -> Result<Script, String> {
    let toks = lex(src)?;
    let complex = toks.iter().any(|t| *t == Tok::Sym(";"));
    let mut p = Parser { t: toks, i: 0 };
    let body = p.statements()?;
    if p.i != p.t.len() {
        return Err(format!("unparsed tail {:?} in {src}", &p.t[p.i..]));
    }
    Ok(Script { body, complex })
}

/// Compile, and fall back to 0 (as Bedrock does) for a broken expression.
pub fn compile_or_zero(src: &str) -> Script {
    compile(src).unwrap_or_else(|_| Script::constant(0.0))
}

// ---------------------------------------------------------------- runtime

/// What a script asks of the game: queries, and the entities `->` names.
pub trait Env {
    fn query(&mut self, scope: &mut Scope, name: &str, args: &[Value]) -> Value;
    /// Evaluate `rhs` as entity `target` (`v.rider->q.is_sneaking`).
    fn arrow(&mut self, _target: u32, _rhs: &Expr, _scope: &mut Scope) -> Value {
        Value::ZERO
    }
}

/// An `Env` with no game behind it: every query is 0.
pub struct NoEnv;
impl Env for NoEnv {
    fn query(&mut self, _: &mut Scope, _: &str, _: &[Value]) -> Value {
        Value::ZERO
    }
}

#[derive(Default, Clone, Debug)]
pub struct Scope {
    pub vars: crate::FastMap<Name, Value>,
    pub temps: crate::FastMap<Name, Value>,
    pub context: crate::FastMap<Name, Value>,
    pub arrays: crate::FastMap<Rc<str>, Vec<Value>>,
    pub this: f32,
    pub rng: u64,
}

enum Ctrl {
    Return(Value),
    Break,
    Continue,
}

const LOOP_CAP: i32 = 1024;

impl Scope {
    pub fn new(seed: u64) -> Scope {
        Scope { rng: seed | 1, ..Default::default() }
    }
    pub fn var(&self, name: &str) -> Option<&Value> {
        self.vars.get(&intern(name))
    }
    pub fn set_var(&mut self, name: &str, v: Value) {
        self.vars.insert(intern(name), v);
    }
    fn table(&mut self, k: VarKind) -> &mut crate::FastMap<Name, Value> {
        match k {
            VarKind::Variable => &mut self.vars,
            VarKind::Temp => &mut self.temps,
            VarKind::Context => &mut self.context,
        }
    }
    fn rand(&mut self) -> f32 {
        // xorshift64*: Math.random needs no quality, just no allocation.
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        ((x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
    }

    /// Runs a script; temps live for one script run, as in Bedrock.
    /// Evaluates one expression (the right side of `->` run as this scope's entity).
    pub fn eval_expr(&mut self, e: &Expr, env: &mut dyn Env) -> Value {
        match self.eval(e, env) {
            Ok(v) | Err(Ctrl::Return(v)) => v,
            Err(_) => Value::ZERO,
        }
    }
    pub fn run(&mut self, s: &Script, env: &mut dyn Env) -> Value {
        self.temps.clear();
        self.run_keep_temps(s, env)
    }
    /// Runs a script sharing temps with the scripts before it (an
    /// animation's channels share `t.` within one evaluation).
    pub fn run_keep_temps(&mut self, s: &Script, env: &mut dyn Env) -> Value {
        if !s.complex {
            if let [e] = s.body.as_slice() {
                return match self.eval(e, env) {
                    Ok(v) | Err(Ctrl::Return(v)) => v,
                    Err(_) => Value::ZERO,
                };
            }
        }
        for e in &s.body {
            match self.eval(e, env) {
                Ok(_) => {}
                Err(Ctrl::Return(v)) => return v,
                Err(_) => return Value::ZERO,
            }
        }
        Value::ZERO
    }
    pub fn num(&mut self, s: &Script, env: &mut dyn Env) -> f32 {
        if let Some(c) = s.as_const() {
            return c;
        }
        self.run(s, env).num()
    }

    fn eval_opt(&mut self, e: &Expr, env: &mut dyn Env) -> Result<Option<Value>, Ctrl> {
        match e {
            Expr::Var(k, n) => Ok(self.table(*k).get(n).cloned()),
            Expr::Index(name, i) => {
                let i = self.eval(i, env)?.num();
                Ok(self.index(name, i))
            }
            other => self.eval(other, env).map(Some),
        }
    }
    fn index(&self, name: &str, i: f32) -> Option<Value> {
        let a = self.arrays.get(name)?;
        if a.is_empty() {
            return None;
        }
        // Bedrock clamps array indices and floors fractions.
        let i = (i.max(0.0) as usize).min(a.len() - 1);
        Some(a[i].clone())
    }

    fn eval(&mut self, e: &Expr, env: &mut dyn Env) -> Result<Value, Ctrl> {
        Ok(match e {
            Expr::Num(n) => Value::Num(*n),
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::This => Value::Num(self.this),
            Expr::Ref(r) => Value::Str(r.clone()),
            Expr::Var(..) | Expr::Index(..) => self.eval_opt(e, env)?.unwrap_or(Value::ZERO),
            Expr::Query(name, args) => {
                let mut a = Vec::with_capacity(args.len());
                for x in args {
                    a.push(self.eval(x, env)?);
                }
                env.query(self, name, &a)
            }
            Expr::Math(f, args) => {
                let mut a = [0f32; 3];
                for (i, x) in args.iter().take(3).enumerate() {
                    a[i] = self.eval(x, env)?.num();
                }
                Value::Num(self.math(*f, a))
            }
            Expr::Neg(x) => Value::Num(-self.eval(x, env)?.num()),
            Expr::Not(x) => Value::bool(!self.eval(x, env)?.truthy()),
            Expr::Bin(op, a, b) => {
                let a = self.eval(a, env)?;
                let b = self.eval(b, env)?;
                match op {
                    Op::Eq => Value::bool(eq(&a, &b)),
                    Op::Ne => Value::bool(!eq(&a, &b)),
                    _ => {
                        let (x, y) = (a.num(), b.num());
                        Value::Num(match op {
                            Op::Add => x + y,
                            Op::Sub => x - y,
                            Op::Mul => x * y,
                            // Bedrock divides by zero to 0, not infinity.
                            Op::Div => {
                                if y == 0.0 { 0.0 } else { x / y }
                            }
                            Op::Lt => (x < y) as i32 as f32,
                            Op::Le => (x <= y) as i32 as f32,
                            Op::Gt => (x > y) as i32 as f32,
                            Op::Ge => (x >= y) as i32 as f32,
                            Op::Eq | Op::Ne => unreachable!(),
                        })
                    }
                }
            }
            Expr::And(a, b) => Value::bool(self.eval(a, env)?.truthy() && self.eval(b, env)?.truthy()),
            Expr::Or(a, b) => Value::bool(self.eval(a, env)?.truthy() || self.eval(b, env)?.truthy()),
            Expr::Coalesce(a, b) => match self.eval_opt(a, env)? {
                Some(v) => v,
                None => self.eval(b, env)?,
            },
            Expr::Ternary(c, a, b) => {
                if self.eval(c, env)?.truthy() { self.eval(a, env)? } else { self.eval(b, env)? }
            }
            Expr::If(c, a) => {
                if self.eval(c, env)?.truthy() { self.eval(a, env)? } else { Value::ZERO }
            }
            Expr::Assign(k, n, x) => {
                let v = self.eval(x, env)?;
                if let Value::Vec3(c) = v {
                    let base = name_str(*n);
                    for (i, a) in ["x", "y", "z"].iter().enumerate() {
                        let f = intern(&format!("{base}.{a}"));
                        self.table(*k).insert(f, Value::Num(c[i]));
                    }
                }
                if let Value::Trs(c) = v {
                    let base = name_str(*n);
                    for (j, part) in ["t", "r", "s"].iter().enumerate() {
                        for (i, a) in ["x", "y", "z"].iter().enumerate() {
                            let f = intern(&format!("{base}.{part}.{a}"));
                            self.table(*k).insert(f, Value::Num(c[j * 3 + i]));
                        }
                    }
                }
                self.table(*k).insert(*n, v.clone());
                v
            }
            Expr::Arrow(a, b) => match self.eval(a, env)? {
                Value::Actor(id) => env.arrow(id, b, self),
                _ => Value::ZERO,
            },
            Expr::Block(stmts) => {
                for s in stmts {
                    self.eval(s, env)?;
                }
                Value::ZERO
            }
            Expr::Return(x) => return Err(Ctrl::Return(self.eval(x, env)?)),
            Expr::Break => return Err(Ctrl::Break),
            Expr::Continue => return Err(Ctrl::Continue),
            Expr::Loop(n, body) => {
                let n = (self.eval(n, env)?.num() as i32).clamp(0, LOOP_CAP);
                for _ in 0..n {
                    match self.eval(body, env) {
                        Ok(_) | Err(Ctrl::Continue) => {}
                        Err(Ctrl::Break) => break,
                        Err(r) => return Err(r),
                    }
                }
                Value::ZERO
            }
            Expr::ForEach(k, n, coll, body) => {
                let items: Vec<Value> = match &**coll {
                    Expr::Index(name, _) => self.arrays.get(name).cloned().unwrap_or_default(),
                    other => vec![self.eval(other, env)?],
                };
                for it in items.into_iter().take(LOOP_CAP as usize) {
                    self.table(*k).insert(*n, it);
                    match self.eval(body, env) {
                        Ok(_) | Err(Ctrl::Continue) => {}
                        Err(Ctrl::Break) => break,
                        Err(r) => return Err(r),
                    }
                }
                Value::ZERO
            }
        })
    }

    fn math(&mut self, f: MathFn, a: [f32; 3]) -> f32 {
        use MathFn::*;
        let [x, y, z] = a;
        let deg = std::f32::consts::PI / 180.0;
        match f {
            // Bedrock's trigonometry works in degrees.
            Abs => x.abs(),
            Acos => x.acos() / deg,
            Asin => x.asin() / deg,
            Atan => x.atan() / deg,
            Atan2 => x.atan2(y) / deg,
            Ceil => x.ceil(),
            Clamp => x.max(y).min(z),
            Cos => (x * deg).cos(),
            Sin => (x * deg).sin(),
            CopySign => x.abs().copysign(y),
            DieRoll => {
                let mut s = 0.0;
                for _ in 0..(x as i32).clamp(0, LOOP_CAP) {
                    s += y + self.rand() * (z - y);
                }
                s
            }
            DieRollInteger => {
                let mut s = 0.0;
                for _ in 0..(x as i32).clamp(0, LOOP_CAP) {
                    s += (y + (self.rand() * (z - y + 1.0)).floor()).min(z);
                }
                s
            }
            Exp => x.exp(),
            Floor => x.floor(),
            HermiteBlend => 3.0 * x * x - 2.0 * x * x * x,
            InverseLerp => {
                if x == y { 0.0 } else { (z - x) / (y - x) }
            }
            Lerp => x + (y - x) * z,
            LerpRotate => {
                let d = wrap180(y - x);
                x + d * z
            }
            Ln => x.ln(),
            Max => x.max(y),
            Min => x.min(y),
            MinAngle => wrap180(x),
            // Java's `%`: the sign follows the dividend.
            Mod => {
                if y == 0.0 { 0.0 } else { x % y }
            }
            Pi => std::f32::consts::PI,
            Pow => x.powf(y),
            Random => x + self.rand() * (y - x),
            RandomInteger => (x + (self.rand() * (y - x + 1.0)).floor()).min(y),
            Round => x.round(),
            Sign => {
                if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 }
            }
            Sqrt => x.sqrt(),
            Trunc => x.trunc(),
            MathFn::Ease(e) => ease(e, x, y, z),
        }
    }
}

fn wrap180(mut d: f32) -> f32 {
    d %= 360.0;
    if d >= 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

fn eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Str(x), Value::Str(y)) => x.eq_ignore_ascii_case(y),
        (Value::Str(_), _) | (_, Value::Str(_)) => false,
        (Value::Actor(x), Value::Actor(y)) => x == y,
        _ => a.num() == b.num(),
    }
}

/// `math.ease_*(start, end, t)`.
pub fn ease(e: Ease, start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * ease01(e, t.clamp(0.0, 1.0))
}

pub fn ease01(e: Ease, t: f32) -> f32 {
    use std::f32::consts::PI;
    use Ease::*;
    let c1 = 1.70158;
    let c2 = c1 * 1.525;
    let c3 = c1 + 1.0;
    let c4 = (2.0 * PI) / 3.0;
    let c5 = (2.0 * PI) / 4.5;
    let out_bounce = |x: f32| {
        let (n1, d1) = (7.5625, 2.75);
        if x < 1.0 / d1 {
            n1 * x * x
        } else if x < 2.0 / d1 {
            let x = x - 1.5 / d1;
            n1 * x * x + 0.75
        } else if x < 2.5 / d1 {
            let x = x - 2.25 / d1;
            n1 * x * x + 0.9375
        } else {
            let x = x - 2.625 / d1;
            n1 * x * x + 0.984375
        }
    };
    match e {
        InQuad => t * t,
        OutQuad => 1.0 - (1.0 - t) * (1.0 - t),
        InOutQuad => {
            if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 }
        }
        InCubic => t * t * t,
        OutCubic => 1.0 - (1.0 - t).powi(3),
        InOutCubic => {
            if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
        }
        InQuart => t.powi(4),
        OutQuart => 1.0 - (1.0 - t).powi(4),
        InOutQuart => {
            if t < 0.5 { 8.0 * t.powi(4) } else { 1.0 - (-2.0 * t + 2.0).powi(4) / 2.0 }
        }
        InQuint => t.powi(5),
        OutQuint => 1.0 - (1.0 - t).powi(5),
        InOutQuint => {
            if t < 0.5 { 16.0 * t.powi(5) } else { 1.0 - (-2.0 * t + 2.0).powi(5) / 2.0 }
        }
        InSine => 1.0 - (t * PI / 2.0).cos(),
        OutSine => (t * PI / 2.0).sin(),
        InOutSine => -((PI * t).cos() - 1.0) / 2.0,
        InExpo => {
            if t == 0.0 { 0.0 } else { 2f32.powf(10.0 * t - 10.0) }
        }
        OutExpo => {
            if t == 1.0 { 1.0 } else { 1.0 - 2f32.powf(-10.0 * t) }
        }
        InOutExpo => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else if t < 0.5 {
                2f32.powf(20.0 * t - 10.0) / 2.0
            } else {
                (2.0 - 2f32.powf(-20.0 * t + 10.0)) / 2.0
            }
        }
        InCirc => 1.0 - (1.0 - t * t).sqrt(),
        OutCirc => (1.0 - (t - 1.0).powi(2)).sqrt(),
        InOutCirc => {
            if t < 0.5 {
                (1.0 - (1.0 - (2.0 * t).powi(2)).sqrt()) / 2.0
            } else {
                ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0) / 2.0
            }
        }
        InBack => c3 * t * t * t - c1 * t * t,
        OutBack => 1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2),
        InOutBack => {
            if t < 0.5 {
                ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0
            } else {
                ((2.0 * t - 2.0).powi(2) * ((c2 + 1.0) * (t * 2.0 - 2.0) + c2) + 2.0) / 2.0
            }
        }
        InElastic => {
            if t == 0.0 || t == 1.0 { t } else { -(2f32.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * c4).sin() }
        }
        OutElastic => {
            if t == 0.0 || t == 1.0 { t } else { 2f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0 }
        }
        InOutElastic => {
            if t == 0.0 || t == 1.0 {
                t
            } else if t < 0.5 {
                -(2f32.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0
            } else {
                (2f32.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0 + 1.0
            }
        }
        InBounce => 1.0 - out_bounce(1.0 - t),
        OutBounce => out_bounce(t),
        InOutBounce => {
            if t < 0.5 { (1.0 - out_bounce(1.0 - 2.0 * t)) / 2.0 } else { (1.0 + out_bounce(2.0 * t - 1.0)) / 2.0 }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Q;
    impl Env for Q {
        fn query(&mut self, _: &mut Scope, name: &str, args: &[Value]) -> Value {
            match name {
                "life_time" => Value::Num(2.0),
                "is_alive" => Value::Num(1.0),
                "is_name_any" => Value::bool(args.iter().any(|a| a.as_str() == Some("Rotty"))),
                _ => Value::ZERO,
            }
        }
    }

    fn run(src: &str, s: &mut Scope) -> f32 {
        s.run(&compile(src).unwrap(), &mut Q).num()
    }

    #[test]
    fn arithmetic_and_precedence() {
        let mut s = Scope::new(1);
        assert_eq!(run("1+2*3", &mut s), 7.0);
        assert_eq!(run("-2*-3 - 1", &mut s), 5.0);
        assert_eq!(run("1 < 2 && 3 >= 3", &mut s), 1.0);
        assert_eq!(run("Math.clamp(5,0,1) + Math.mod(-7,3)", &mut s), 0.0);
        assert!((run("Math.sin(90)", &mut s) - 1.0).abs() < 1e-6);
        assert_eq!(run("1/0", &mut s), 0.0);
    }

    #[test]
    fn statements_return_and_variables() {
        let mut s = Scope::new(1);
        assert_eq!(run("v.a = 3; t.b = v.a * 2; return t.b + 1;", &mut s), 7.0);
        assert_eq!(s.var("a"), Some(&Value::Num(3.0)));
        // A program without return is 0.
        assert_eq!(run("v.a = 4;", &mut s), 0.0);
        assert_eq!(run("v.a", &mut s), 4.0);
    }

    #[test]
    fn coalesce_ternary_and_binary_if() {
        let mut s = Scope::new(1);
        assert_eq!(run("v.missing ?? 5", &mut s), 5.0);
        assert_eq!(run("(v.drqdfi??0) ? 1 : 2", &mut s), 2.0);
        assert_eq!(run("q.is_alive ? {v.x = 9;}; return v.x;", &mut s), 9.0);
        assert_eq!(run("q.is_alive ? 0 : 1 ? 2 : 3", &mut s), 0.0);
    }

    #[test]
    fn loops_and_strings() {
        let mut s = Scope::new(1);
        assert_eq!(run("t.i = 0; loop(10, {t.i = t.i + 1; t.i >= 4 ? break;}); return t.i;", &mut s), 4.0);
        assert_eq!(run("v.s = 'diamond'; return v.s == 'Diamond';", &mut s), 1.0);
        assert_eq!(run("q.is_name_any('baby','Rotty')", &mut s), 1.0);
    }

    #[test]
    fn real_zombie_scripts_compile() {
        let src = "t.uml=(!v.cdrzno)?1:0;!q.is_in_ui&&!(v.is_paperdoll??0)?{t.cuq=t.uml+(q.is_alive?0:2)+(q.death_ticks>0?4:0);t.cuq!=v.wpiedr?{v.wpiedr=t.cuq;v.qcllpd=q.life_time+0.15;};};v.dxvpbr=t.uml&&q.life_time>=v.qcllpd;";
        let mut s = Scope::new(1);
        run(src, &mut s);
        assert_eq!(s.var("wpiedr"), Some(&Value::Num(1.0)));
        // life_time 2 < qcllpd 2.15: the 0.15 s settle delay has not passed yet.
        assert_eq!(s.var("dxvpbr"), Some(&Value::Num(0.0)));
        compile("v.rpqxbs==1?Geometry.hjrztg:v.kcrehx?Geometry.mpselt:Geometry.baby").unwrap();
        compile("Math.cos(q.life_time*1200)*-0.35+0.35").unwrap();
        compile("v.r->q.is_sneaking").unwrap();
    }

    #[test]
    fn structs_flatten_on_assignment() {
        struct B;
        impl Env for B {
            fn query(&mut self, _: &mut Scope, _: &str, _: &[Value]) -> Value { Value::Vec3([1.0, 24.0, 3.0]) }
        }
        let mut s = Scope::new(1);
        let v = s.run(&compile("t.y=q.bone_origin('root');return t.y.y-24;").unwrap(), &mut B);
        assert_eq!(v.num(), 0.0);
    }

    #[test]
    fn trs_structs_flatten_on_assignment() {
        struct B;
        impl Env for B {
            fn query(&mut self, _: &mut Scope, _: &str, _: &[Value]) -> Value {
                Value::Trs([1.0, 2.0, 3.0, 10.0, 20.0, 30.0, 1.0, 1.0, 1.0])
            }
        }
        let mut s = Scope::new(1);
        let v = s.run(&compile("v.a=q.bone_orientation_trs('leg');return v.a.r.y+v.a.t.z;").unwrap(), &mut B);
        assert_eq!(v.num(), 23.0);
    }

    #[test]
    fn arrays_clamp() {
        let mut s = Scope::new(1);
        s.arrays.insert(Rc::from("array.skins"), vec![Value::str("a"), Value::str("b")]);
        let v = s.run(&compile("Array.skins[7]").unwrap(), &mut Q);
        assert_eq!(v.as_str(), Some("b"));
    }
}
