use crate::cache;
use crate::categories::Category;
use crate::collections::Collection;
use crate::common::{open_file_with_vim, system_time_as_unix_time};
use crate::concept::{AttributeId, ConceptId};
use crate::paths;
use crate::reviews::{Recall, Review, Reviews};
use crate::{common::current_time, common::Id};
use rayon::prelude::*;
use samsvar::json;
use samsvar::Matcher;
use sanitize_filename::sanitize;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fs::{self, create_dir_all, read_to_string};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::card::Card;
use crate::card::CardLocation;
use crate::card::IsSuspended;

use super::{BackSide, CardType, RawCard, RecallRate};

/// Represents a card that has been saved as a toml file, which is basically anywhere in the codebase
/// except for when youre constructing a new card.
/// Don't save this in containers or pass to functions, rather use the Id, and get new instances of SavedCard from the cache.
/// Also, every time you mutate it, call the persist() method.
#[derive(Clone, Ord, PartialOrd, PartialEq, Eq, Hash, Debug)]
pub struct SavedCard {
    card: Card,
    history: Reviews,
    location: CardLocation,
    last_modified: Duration,
    suspended: IsSuspended,
}

impl std::fmt::Display for SavedCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.card.display())
    }
}

impl From<SavedCard> for Card {
    fn from(value: SavedCard) -> Self {
        value.card
    }
}

/// Associated methods
impl SavedCard {
    pub fn create(data: CardType, category: &Category) -> Self {
        let card = Card::new(data);
        Self::new_at(card, category)
    }

    pub fn new_at(card: Card, category: &Category) -> Self {
        let filename = sanitize(card.display().replace(" ", "_").replace("'", ""));
        let dir = category.as_path();
        create_dir_all(&dir).unwrap();
        let mut path = dir.join(&filename);
        path.set_extension("toml");
        if path.exists() {
            let dir = category.as_path();
            path = dir.join(&card.id.to_string());
            path.set_extension("toml");
        };

        let raw_card = RawCard::from_card(card);

        let s: String = toml::to_string_pretty(&raw_card).unwrap();

        let mut file = fs::File::create_new(&path).unwrap();

        file.write_all(&mut s.as_bytes()).unwrap();

        Self::from_path(&path)
    }

    pub fn new(card: Card) -> Self {
        Self::new_at(card, &Category::default())
    }

    fn get_cards_from_categories(cats: Vec<Category>) -> Vec<Self> {
        cats.into_par_iter()
            .flat_map(|cat| {
                cat.get_containing_card_paths()
                    .into_par_iter()
                    .map(|path| Self::from_path(&path))
                    .collect::<Vec<Self>>()
            })
            .collect()
    }

    // potentially expensive function!
    pub fn from_id(id: &Id) -> Option<Self> {
        let path = cache::path_from_id(*id)?;
        Self::from_path(&path).into()
    }

    pub fn xload_pending(filter: Option<String>) -> Vec<Id> {
        let mut cards = Self::load_all_cards();

        cards.retain(|card| card.history.is_empty());

        if let Some(filter) = filter {
            cards.retain(|card| card.eval(filter.clone()));
        }

        cards.iter().map(|card| card.id()).collect()
    }

    pub fn load_pending(filter: Option<String>) -> Vec<Id> {
        Self::load_all_cards()
            .into_par_iter()
            .filter(|card| card.history.is_empty())
            .filter(|card| {
                if let Some(ref filter) = filter {
                    card.eval(filter.clone())
                } else {
                    true
                }
            })
            .map(|card| card.id())
            .collect()
    }

    pub fn load_non_pending(filter: Option<String>) -> Vec<Id> {
        Self::load_all_cards()
            .into_par_iter()
            .filter(|card| !card.history.is_empty())
            .filter(|card| {
                if let Some(ref filter) = filter {
                    card.eval(filter.clone())
                } else {
                    true
                }
            })
            .map(|card| card.id())
            .collect()
    }

    pub fn load_all_cards() -> Vec<Self> {
        let collections = Collection::load_all();

        let mut categories: Vec<Category> = collections
            .into_par_iter()
            .flat_map(|col| col.load_categories())
            .collect();

        let extra_categories = Category::load_all(None);
        categories.extend(extra_categories);

        Self::get_cards_from_categories(categories)
    }

    pub fn from_path(path: &Path) -> Self {
        let content = read_to_string(path).expect("Could not read the TOML file");
        let Ok(raw_card) = toml::from_str::<RawCard>(&content) else {
            dbg!("faild to read card from path: ", path);
            panic!();
        };

        let Some(card) = raw_card.into_card() else {
            println!("{}", path.display());
            panic!();
        };

        let location = CardLocation::new(path);

        let last_modified = {
            let system_time = std::fs::metadata(path).unwrap().modified().unwrap();
            system_time_as_unix_time(system_time)
        };

        let history: Reviews = {
            let path = paths::get_review_path().join(card.id.to_string());
            if path.exists() {
                let s = fs::read_to_string(path).unwrap();
                Reviews::from_str(&s)
            } else {
                Default::default()
            }
        };

        Self {
            card,
            location,
            last_modified,
            history,
            suspended: IsSuspended::default(),
        }
    }
}

impl SavedCard {
    pub fn save_new_reviews(&self) {
        if self.history.is_empty() {
            return;
        }
        self.history.save(self.id());
    }

    pub fn set_ref(&mut self, id: Id) {
        match &mut self.card.data {
            CardType::Normal { ref mut back, .. } => {
                *back = BackSide::Card(id);
            }
            CardType::Concept { .. } => return,
            CardType::Attribute { ref mut back, .. } => {
                *back = BackSide::Card(id);
            }
            CardType::Unfinished { .. } => return,
        }

        self.persist();
    }

    fn time_passed_since_last_review(&self) -> Option<Duration> {
        if current_time() < self.history.0.last()?.timestamp {
            return Duration::default().into();
        }

        Some(current_time() - self.history.0.last()?.timestamp)
    }

    pub fn save_reviews(&self) {
        let s: String = serde_json::to_string_pretty(self.reviews()).unwrap();
        let path = paths::get_review_path().join(self.id().to_string());
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(&mut s.as_bytes()).unwrap();
    }

    pub fn recall_rate_at(&self, current_unix: Duration) -> Option<RecallRate> {
        crate::recall_rate::recall_rate(&self.history, current_unix)
    }
    pub fn recall_rate(&self) -> Option<RecallRate> {
        let now = current_time();
        crate::recall_rate::recall_rate(&self.history, now)
    }

    pub fn rm_dependency(&mut self, dependency: Id) -> bool {
        let res = self.card.dependencies.remove(&dependency);
        self.persist();
        res
    }

    pub fn set_attribute(&mut self, id: AttributeId, concept_card: Id) {
        let back = self.back_side().unwrap().to_owned();
        let data = CardType::Attribute {
            back,
            attribute: id,
            concept_card,
        };
        self.card.data = data;
        self.persist();
    }

    pub fn set_concept(&mut self, concept: ConceptId) {
        let name = self.card.display();
        let ty = CardType::Concept { name, concept };
        self.card.data = ty;
        self.persist();
    }

    pub fn card_type(&self) -> &CardType {
        &self.card.data
    }

    pub fn set_dependency(&mut self, dependency: Id) {
        if self.id() == dependency {
            return;
        }
        self.card.dependencies.insert(dependency);
        self.persist();
        cache::add_dependent(dependency, self.id());
    }

    fn is_resolved(&self) -> bool {
        for id in self.all_dependencies() {
            if let Some(card) = SavedCard::from_id(&id) {
                if !card.is_finished() {
                    return false;
                }
            }
        }

        true
    }

    fn all_dependencies(&self) -> Vec<Id> {
        fn inner(id: Id, deps: &mut Vec<Id>) {
            let Some(card) = SavedCard::from_id(&id) else {
                return;
            };

            for dep in card.dependency_ids() {
                deps.push(dep);
                inner(dep, deps);
            }
        }

        let mut deps = vec![];

        inner(self.id(), &mut deps);

        deps
    }

    pub fn maturity(&self) -> f32 {
        use gkquad::single::integral;

        let now = current_time();
        let result = integral(
            |x: f64| {
                self.recall_rate_at(now + Duration::from_secs_f64(x * 86400.))
                    .unwrap_or_default() as f64
            },
            0.0..1000.,
        )
        .estimate()
        .unwrap();

        result as f32
    }

    pub fn print(&self) -> String {
        self.card.display()
    }

    pub fn reviews(&self) -> &Vec<Review> {
        &self.history.0
    }

    pub fn last_modified(&self) -> Duration {
        self.last_modified
    }

    pub fn category(&self) -> &Category {
        &self.location.category
    }

    #[allow(dead_code)]
    pub fn is_pending(&self) -> bool {
        self.history.is_empty()
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended.is_suspended()
    }

    pub fn is_finished(&self) -> bool {
        !matches!(self.card.card_type(), CardType::Unfinished { .. })
    }

    pub fn time_since_last_review(&self) -> Option<Duration> {
        self.time_passed_since_last_review()
    }

    pub fn id(&self) -> Id {
        self.card.id
    }

    pub fn dependency_ids(&self) -> BTreeSet<Id> {
        let mut deps = self.card.dependencies.clone();
        deps.extend(self.card.data.dependencies());
        deps
    }

    pub fn as_path(&self) -> PathBuf {
        self.location.as_path()
    }

    /// Checks if corresponding file has been modified after this type got deserialized from the file.
    pub fn is_outdated(&self) -> bool {
        let file_last_modified = {
            let path = self.as_path();
            let system_time = std::fs::metadata(path).unwrap().modified().unwrap();
            system_time_as_unix_time(system_time)
        };

        let in_memory_last_modified = self.last_modified;

        match in_memory_last_modified.cmp(&file_last_modified) {
            Ordering::Less => true,
            Ordering::Equal => false,
            Ordering::Greater => panic!("Card in-memory shouldn't have a last_modified more recent than its corresponding file"),
        }
    }

    pub fn edit_with_vim(&self) -> Self {
        let path = self.as_path();
        open_file_with_vim(path.as_path()).unwrap();
        Self::from_path(path.as_path())
    }

    // Call this function every time SavedCard is mutated.
    pub fn persist(&mut self) {
        if self.is_outdated() {
            // When you persist, the last_modified in the card should match the ones from the file.
            // This shouldn't be possible, as this function mutates itself to get a fresh copy, so
            // i'll panic here to alert me of the logic bug.
            let _x = format!("{:?}", self);
            // panic!("{}", x);
        }

        let path = self.as_path();
        if !path.exists() {
            let msg = format!("following path doesn't really exist: {}", path.display());
            panic!("{msg}");
        }

        self.history.save(self.id());
        let raw_card = RawCard::from_card(self.card.clone());
        let toml = toml::to_string(&raw_card).unwrap();

        std::fs::write(&path, toml).unwrap();
        *self = SavedCard::from_path(path.as_path())
    }

    pub fn new_review(&mut self, grade: Recall, time: Duration) {
        let review = Review::new(grade, time);
        self.history.add_review(review);
        self.persist();
    }

    pub fn lapses(&self) -> u32 {
        self.history.lapses()
    }

    pub fn concept(&self) -> Option<ConceptId> {
        if let CardType::Concept { concept, .. } = self.card_type() {
            Some(*concept)
        } else {
            None
        }
    }

    pub fn back_side(&self) -> Option<&BackSide> {
        match self.card_type() {
            CardType::Normal { back, .. } => Some(back),
            CardType::Concept { .. } => None?,
            CardType::Attribute { back, .. } => Some(back),
            CardType::Unfinished { .. } => None?,
        }
    }

    pub fn set_type_normal(&mut self, front: String, back: String) {
        let data = CardType::Normal {
            front,
            back: back.into(),
        };
        self.card.data = data;
        self.persist();
    }
}

impl Matcher for SavedCard {
    fn get_val(&self, key: &str) -> Option<samsvar::Value> {
        match key {
            "front" => json!(&self.card.display()),
            "back" => json!(&self
                .back_side()
                .map(|bs| bs.to_string())
                .unwrap_or_default()),
            "suspended" => json!(&self.is_suspended()),
            "finished" => json!(&self.is_finished()),
            "resolved" => json!(&self.is_resolved()),
            "id" => json!(&self.id().to_string()),
            "recall" => json!(self.recall_rate().unwrap_or_default()),
            "stability" => json!(self.maturity()),
            "lapses" => json!(self.lapses()),
            "lastreview" => json!(
                self.time_since_last_review()
                    .unwrap_or_else(|| Duration::MAX)
                    .as_secs_f32()
                    / 86400.
            ),
            "minrecrecall" => {
                let mut min_stability = usize::MAX;
                let cards = self.all_dependencies();
                for id in cards {
                    let stab = (SavedCard::from_id(&id)
                        .unwrap()
                        .recall_rate()
                        .unwrap_or_default()
                        * 1000.) as usize;
                    min_stability = min_stability.min(stab);
                }

                json!(min_stability as f32 / 1000.)
            }
            "minrecstab" => {
                let mut min_recall = usize::MAX;
                let cards = self.all_dependencies();
                for id in cards {
                    let stab = (SavedCard::from_id(&id).unwrap().maturity() * 1000.) as usize;
                    min_recall = min_recall.min(stab);
                }

                json!(min_recall as f32 / 1000.)
            }
            "dependencies" => json!(self.dependency_ids().len()),
            "dependents" => {
                let id = self.id();
                let mut count: usize = 0;

                for card in SavedCard::load_all_cards() {
                    if card.dependency_ids().contains(&id) {
                        count += 1;
                    }
                }

                json!(count)
            }
            _ => return None,
        }
        .into()
    }
}
