// Compiles every Molang expression in the original jar's data. Set
// ANS_DATA to the extracted jar's assets/ans folder.
use ans_beyond_engine::{json, molang};
use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() { walk(&p, out) } else if p.extension().is_some_and(|x| x == "json") { out.push(p) }
    }
}

const NOT_MOLANG: &[&str] = &["", "format", "format_version", "min_engine_version", "file", "layer", "bedrock", "animation", "render_type", "lerp_mode", "loop", "id", "identifier", "initial_state", "animation_file", "effect", "locator", "name", "texture", "geometry", "bind_to_actor"];

fn strings(j: &json::Json, key: &str, out: &mut Vec<String>) {
    match j {
        json::Json::Str(s) if !NOT_MOLANG.contains(&key) => out.push(s.clone()),
        json::Json::Arr(a) => a.iter().for_each(|x| strings(x, key, out)),
        json::Json::Obj(kv) => kv.iter().for_each(|(k, v)| strings(v, k, out)),
        _ => {}
    }
}

#[test]
fn every_molang_string_compiles() {
    let Ok(root) = std::env::var("ANS_DATA") else { return };
    let mut files = Vec::new();
    for d in ["entity", "geckolib/animations"] { walk(&Path::new(&root).join(d), &mut files) }
    let (mut n, mut bad) = (0, Vec::new());
    for f in &files {
        let j = json::parse(&std::fs::read_to_string(f).unwrap()).unwrap_or_else(|e| panic!("{f:?}: {e}"));
        let mut s = Vec::new();
        strings(&j, "", &mut s);
        for x in s {
            // identifiers, file references and plain names are not Molang
            if x.contains(':') && !x.contains('?') || x.ends_with(".png") { continue }
            // bone and part names (`8dgo`, `head1`) sit beside expressions in part_visibility
            if x.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'*') && !x.starts_with("v.") { continue }
            n += 1;
            if let Err(e) = molang::compile(&x) { bad.push(format!("{}: {x:.120} -> {e:.120}", f.display())) }
        }
    }
    eprintln!("{n} strings, {} failed", bad.len());
    for b in bad.iter().take(25) { eprintln!("{b}") }
    assert!(bad.is_empty());
}
