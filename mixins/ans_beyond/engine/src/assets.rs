// Loading an entity with its animations and geometry from a resource tree
// laid out as the jar's `assets/ans/` (and the content mod's): entity
// bundles in `entity/`, GeckoLib geometry and animations in `geckolib/`.

use crate::bundle::Bundle;
use crate::geo::{self, Geometry};
use crate::{anim, json};

use std::rc::Rc;

/// Reads `assets/<ns>/<path>` from wherever the resources are.
pub trait Source {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
}

pub struct Dir(pub std::path::PathBuf);
impl Source for Dir {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(path)).ok()
    }
}

fn split(id: &str) -> (&str, &str) {
    id.split_once(':').unwrap_or(("minecraft", id))
}

#[derive(Default)]
pub struct Library {
    bundles: crate::FastMap<String, Option<Rc<Bundle>>>,
    geometry: crate::FastMap<String, Option<Rc<Geometry>>>,
    /// `attachable/_index.json`: per item, its attachables (path, condition).
    index: Option<crate::FastMap<String, Vec<(String, Option<crate::molang::Script>)>>>,
    attachables: crate::FastMap<String, Option<Rc<Bundle>>>,
    /// Animation files many bundles share, parsed once.
    anim_files: crate::FastMap<String, Rc<crate::FastMap<String, Rc<anim::Animation>>>>,
}

impl Library {
    /// Forgets everything loaded (resource packs changed).
    pub fn clear(&mut self) {
        self.bundles.clear();
        self.geometry.clear();
        self.index = None;
        self.attachables.clear();
        self.anim_files.clear();
    }
}

impl Library {
    fn json(src: &dyn Source, path: &str) -> Option<json::Json> {
        let bytes = src.read(path)?;
        json::parse(std::str::from_utf8(&bytes).ok()?).ok()
    }

    /// The bundle for an entity type (`minecraft:zombie`), from
    /// `assets/ans/entity/<name>.json`.
    pub fn bundle(&mut self, src: &dyn Source, entity: &str) -> Option<Rc<Bundle>> {
        if let Some(b) = self.bundles.get(entity) {
            return b.clone();
        }
        let name = crate::ids::bundle_name(entity);
        // `_settings` and friends are not entities.
        if name.starts_with('_') || name.contains('/') || name.contains('.') {
            self.bundles.insert(entity.to_string(), None);
            return None;
        }
        let b = Self::json(src, &format!("assets/ans/entity/{name}.json")).map(|j| {
            let b = Bundle::parse(&j);
            let (ns, path) = split(&b.animation_file);
            if let Some(a) = Self::json(src, &format!("assets/{ns}/geckolib/animations/{path}.animation.json")) {
                b.set_animations(anim::parse_file(&a));
            }
            Rc::new(b)
        });
        self.bundles.insert(entity.to_string(), b.clone());
        b
    }

    /// Geometry by id (`ans:entity/zombie`), from GeckoLib's models folder.
    /// The attachables the jar's index lists for `item` (`minecraft:diamond_sword`):
    /// (bundle path, condition), first match wins.
    pub fn attachables_for(&mut self, src: &dyn Source, item: &str) -> &[(String, Option<crate::molang::Script>)] {
        if self.index.is_none() {
            let mut map = crate::FastMap::default();
            if let Some(j) = Self::json(src, "assets/ans/attachable/_index.json") {
                for (item, list) in j.get("items").map(json::Json::entries).unwrap_or(&[]) {
                    let v = list
                        .as_arr()
                        .iter()
                        .filter_map(|e| {
                            let a = e.as_arr();
                            let path = a.first()?.as_str()?.to_string();
                            let cond = a.get(1).and_then(json::Json::as_str).map(crate::molang::compile_or_zero);
                            Some((path, cond))
                        })
                        .collect();
                    map.insert(item.clone(), v);
                }
            }
            self.index = Some(map);
        }
        self.index.as_ref().and_then(|m| m.get(item)).map_or(&[], Vec::as_slice)
    }

    /// An attachable bundle (`ans:attachable/minecraft/diamond_sword`).
    pub fn attachable(&mut self, src: &dyn Source, id: &str) -> Option<Rc<Bundle>> {
        if let Some(b) = self.attachables.get(id) {
            return b.clone();
        }
        let (ns, path) = split(id);
        let b = Self::json(src, &format!("assets/{ns}/{path}.json")).map(|j| {
            let b = Bundle::parse(&j);
            let file = b.animation_file.clone();
            if !self.anim_files.contains_key(&file) {
                let (ans, apath) = split(&file);
                let parsed: crate::FastMap<String, Rc<anim::Animation>> = Self::json(src, &format!("assets/{ans}/geckolib/animations/{apath}.animation.json"))
                    .map(|a| anim::parse_file(&a).into_iter().map(|(k, v)| (k, Rc::new(v))).collect())
                    .unwrap_or_default();
                self.anim_files.insert(file.clone(), Rc::new(parsed));
            }
            if let Some(a) = self.anim_files.get(&file) {
                b.set_shared_animations(a);
            }
            Rc::new(b)
        });
        self.attachables.insert(id.to_string(), b.clone());
        b
    }

    pub fn geometry(&mut self, src: &dyn Source, id: &str) -> Option<Rc<Geometry>> {
        if let Some(g) = self.geometry.get(id) {
            return g.clone();
        }
        let (ns, path) = split(id);
        let g = Self::json(src, &format!("assets/{ns}/geckolib/models/{path}.geo.json"))
            .and_then(|j| geo::parse_file(&j).into_iter().next())
            .map(Rc::new);
        self.geometry.insert(id.to_string(), g.clone());
        g
    }
}
