use crate::{common::Id, paths, SavedCard};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs::read_to_string, path::PathBuf};

pub fn add_dependent(card: Id, dependent: Id) {
    let mut info = match CacheInfo::load_and_verify(card) {
        Some(info) => info,
        None => {
            sync_cache();
            CacheInfo::load_and_verify(card).unwrap()
        }
    };

    if !info.dependents.contains(&dependent) {
        info.dependents.push(dependent);
    }

    info.save(card);
}

pub fn path_from_id(id: Id) -> Option<PathBuf> {
    match CacheInfo::load_and_verify(id) {
        Some(info) => Some(info.path),
        None => {
            sync_cache();
            CacheInfo::load_and_verify(id)?.path.into()
        }
    }
}

fn sync_cache() {
    let mut infos = HashMap::new();
    let cards = SavedCard::load_all_cards();

    for card in &cards {
        infos.insert(
            card.id(),
            CacheInfo {
                path: card.as_path(),
                dependents: vec![],
            },
        );
    }

    for card in &cards {
        for dependency in card.dependency_ids() {
            if let Some(m) = infos.get_mut(&dependency) {
                m.dependents.push(card.id());
            }
        }
    }

    for (id, info) in infos {
        info.save(id);
    }
}

fn id_path(id: &Id) -> PathBuf {
    paths::get_cache_path().join(id.to_string())
}

#[derive(Debug, Serialize, Deserialize, Hash)]
struct CacheInfo {
    path: PathBuf,
    dependents: Vec<Id>,
}

impl CacheInfo {
    fn save(&self, id: Id) -> CacheInfo {
        let mut s: String = toml::to_string_pretty(self).unwrap();
        let path = id_path(&id);
        std::fs::write(path, &mut s).unwrap();
        Self::load(id).unwrap()
    }

    fn load(id: Id) -> Option<CacheInfo> {
        let path = id_path(&id);
        let info = toml::from_str(&read_to_string(&path).ok()?).ok()?;
        Some(info)
    }

    fn load_and_verify(id: Id) -> Option<CacheInfo> {
        let info = Self::load(id)?;
        info.path.exists().then_some(info)
    }
}
