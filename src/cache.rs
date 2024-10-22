use crate::{common::CardId, paths, SavedCard};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs::read_to_string, path::PathBuf};

pub fn add_dependent(card: CardId, dependent: CardId) {
    if card == dependent {
        return;
    }

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

pub fn dependents_from_id(id: CardId) -> Vec<CardId> {
    match CacheInfo::load_and_verify(id) {
        Some(info) => info.dependents,
        None => {
            sync_cache();
            CacheInfo::load_and_verify(id)
                .map(|info| info.dependents)
                .unwrap_or_default()
        }
    }
}

pub fn path_from_id(id: CardId) -> Option<PathBuf> {
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

fn id_path(id: &CardId) -> PathBuf {
    paths::get_cache_path().join(id.to_string())
}

#[derive(Debug, Serialize, Deserialize, Hash)]
struct CacheInfo {
    path: PathBuf,
    dependents: Vec<CardId>,
}

impl CacheInfo {
    fn save(&self, id: CardId) -> CacheInfo {
        let mut s: String = toml::to_string_pretty(self).unwrap();
        let path = id_path(&id);
        std::fs::write(path, &mut s).unwrap();
        Self::load(id).unwrap()
    }

    fn load(id: CardId) -> Option<CacheInfo> {
        let path = id_path(&id);
        let info = toml::from_str(&read_to_string(&path).ok()?).ok()?;
        Some(info)
    }

    fn load_and_verify(id: CardId) -> Option<CacheInfo> {
        let info = Self::load(id)?;
        info.path.exists().then_some(info)
    }
}
