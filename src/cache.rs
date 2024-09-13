use std::collections::{BTreeSet, HashMap, VecDeque};

use std::sync::Arc;

use crate::card::SavedCard;
use crate::common::Id;
use crate::common::{get_last_modified, system_time_as_unix_time};

#[derive(Debug, Default)]
pub struct CardCache(HashMap<Id, Arc<SavedCard>>);

impl CardCache {
    /// Checks that the card in the cache is up to date, and fixes it if it's not.
    /// There's three possibilities:
    ///     1. its up to date, no need to do anything.
    ///     2. It's outdated, we simply deserialize the card from the same path, so that its updated.
    ///     3. card isn't even found in the location, we search through all the cards to find it. Panicking if it's not found.
    fn maybe_update(&mut self, id: Id) {
        enum CardCacheStatus {
            MissingFromCache,
            FileMissing,
            NeedsUpdate,
            UpToDate,
        }

        let status = match self.0.get(&id) {
            Some(cached_card) => {
                let path = cached_card.as_path();
                if path.exists() {
                    // Get the file's last_modified time
                    let metadata = std::fs::metadata(path.as_path()).unwrap();
                    let last_modified_time = system_time_as_unix_time(metadata.modified().unwrap());

                    if last_modified_time > cached_card.last_modified() {
                        CardCacheStatus::NeedsUpdate
                    } else {
                        CardCacheStatus::UpToDate
                    }
                } else {
                    CardCacheStatus::FileMissing
                }
            }
            None => CardCacheStatus::MissingFromCache,
        };

        match status {
            CardCacheStatus::UpToDate => {}
            CardCacheStatus::NeedsUpdate => {
                let path = self.0.get(&id).unwrap().as_path();
                let updated_card = SavedCard::from_path(path.as_path());
                self.0.insert(id, updated_card.into());
            }
            CardCacheStatus::FileMissing | CardCacheStatus::MissingFromCache => {
                // Read the card from the disk
                // expensive! it'll comb through all the cards linearly.
                if let Some(card) = SavedCard::from_id(&id) {
                    self.0.insert(id, card.into());
                };
            }
        };
    }

    pub fn ids_as_vec(&self) -> Vec<Id> {
        self.0.keys().copied().collect()
    }

    /// gets all the Ids (keys) sorted by recent modified
    pub fn all_ids(&self) -> Vec<Id> {
        let mut pairs: Vec<_> = self.0.iter().collect();
        pairs.sort_by_key(|&(_, v)| {
            if v.is_outdated() {
                get_last_modified(&v.as_path())
            } else {
                v.last_modified().to_owned()
            }
        });
        pairs.reverse();
        pairs.into_iter().map(|(k, _)| k.to_owned()).collect()
    }

    pub fn exists(&self, id: &Id) -> bool {
        self.0.get(id).is_some()
    }

    pub fn insert(&mut self, card: SavedCard) {
        let id = card.id();
        self.0.insert(id, card.into());
    }

    pub fn remove(&mut self, id: Id) {
        self.0.remove(&id);
    }

    pub fn dependencies(&mut self, id: Id) -> BTreeSet<Id> {
        let Some(card) = self.try_get_ref(id) else {
            return Default::default();
        };

        card.dependency_ids()
            .iter()
            .map(ToOwned::to_owned)
            .collect()
    }

    pub fn recursive_dependencies(&mut self, id: Id) -> BTreeSet<Id> {
        let mut dependencies = BTreeSet::new();
        let mut stack = VecDeque::new();
        stack.push_back(id);

        while let Some(card) = stack.pop_back() {
            if !dependencies.contains(&card) {
                dependencies.insert(card);

                let card_dependencies = self.dependencies(card);

                for dependency in card_dependencies {
                    stack.push_back(dependency);
                }
            }
        }

        dependencies.remove(&id);
        dependencies
    }

    pub fn try_get_ref(&mut self, id: Id) -> Option<Arc<SavedCard>> {
        self.maybe_update(id);
        self.0.get(&id).cloned()
    }

    pub fn get_owned(&mut self, id: Id) -> SavedCard {
        (*self.get_ref(id)).clone()
    }

    pub fn get_ref(&mut self, id: Id) -> Arc<SavedCard> {
        self.try_get_ref(id).unwrap()
    }

    pub fn new() -> Self {
        let mut cache = Self::default();
        cache.cache_all();
        cache
    }

    pub fn refresh(&mut self) {
        *self = Self::new();
    }

    fn cache_all(&mut self) {
        let all_cards = SavedCard::load_all_cards();
        for card in all_cards {
            self.cache_one(card);
        }
    }

    pub fn cache_one(&mut self, card: SavedCard) {
        self.0.insert(card.id(), card.into());
    }

    pub fn new_empty() -> Self {
        CardCache(HashMap::new())
    }
}
