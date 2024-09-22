use crate::cache;
use crate::categories::Category;
use crate::common::{open_file_with_vim, system_time_as_unix_time};
use crate::paths;
use crate::reviews::{Grade, Review, Reviews};
use crate::{common::current_time, common::Id};
use samsvar::json;
use samsvar::Matcher;
use sanitize_filename::sanitize;
use serde::de::Deserializer;
use serde::{de, Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{self, create_dir_all, read_to_string};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use toml::Value;
use uuid::Uuid;

pub type RecallRate = f32;

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Debug)]
pub struct CardLocation {
    file_name: OsString,
    category: Category,
}

impl CardLocation {
    pub fn new(path: &Path) -> Self {
        let file_name = path.file_name().unwrap().to_owned();
        let category = Category::from_card_path(path);
        Self {
            file_name,
            category,
        }
    }

    fn as_path(&self) -> PathBuf {
        let mut path = self.category.as_path().join(self.file_name.clone());
        path.set_extension("toml");
        path
    }
}

impl std::fmt::Display for SavedCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.front_text())
    }
}

impl From<SavedCard> for Card {
    fn from(value: SavedCard) -> Self {
        value.card
    }
}

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

/// Associated methods
impl SavedCard {
    pub fn new(card: Card) -> Self {
        let filename = sanitize(card.front.clone().replace(" ", "_").replace("'", ""));
        let dir = Category::private().as_path();
        create_dir_all(&dir).unwrap();
        let mut path = dir.join(&filename);
        path.set_extension("toml");
        if path.exists() {
            path = crate::paths::get_cards_path().join(&card.id.to_string());
            path.set_extension("toml");
        };

        let s: String = toml::to_string_pretty(&card).unwrap();

        let mut file = fs::File::create_new(&path).unwrap();

        file.write_all(&mut s.as_bytes()).unwrap();

        Self::from_path(&path)
    }

    fn get_cards_from_categories(cats: Vec<Category>) -> Vec<Self> {
        let mut cards = vec![];

        for cat in cats {
            for path in cat.get_containing_card_paths() {
                dbg!(&path);
                let card = Self::from_path(&path);
                cards.push(card);
            }
        }

        cards
    }

    pub fn get_cards_from_category_recursively(category: &Category) -> Vec<Self> {
        let categories = category.get_following_categories();
        Self::get_cards_from_categories(categories)
    }

    // expensive function!
    pub fn from_id(id: &Id) -> Option<Self> {
        let path = cache::path_from_id(*id)?;
        Self::from_path(&path).into()
    }

    pub fn load_all_cards() -> Vec<Self> {
        let categories = dbg!(Category::load_all());
        Self::get_cards_from_categories(categories)
    }

    pub fn from_path(path: &Path) -> Self {
        dbg!(&path);
        let content = read_to_string(path).expect("Could not read the TOML file");
        let card: Card = toml::from_str(&content).unwrap();
        let location = CardLocation::new(path);
        dbg!();

        let last_modified = {
            let system_time = std::fs::metadata(path).unwrap().modified().unwrap();
            system_time_as_unix_time(system_time)
        };
        dbg!();

        let history: Reviews = {
            let path = paths::get_review_path().join(card.id.to_string());
            if path.exists() {
                let s = fs::read_to_string(path).unwrap();
                serde_json::from_str(&s).unwrap()
            } else {
                Default::default()
            }
        };
        dbg!();

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

    pub fn recall_rate(&self) -> Option<RecallRate> {
        crate::recall_rate::recall_rate(&self.history)
    }

    pub fn rm_dependency(&mut self, dependency: Id) -> bool {
        let res = self.card.dependencies.remove(&dependency);
        self.persist();
        res
    }

    pub fn set_dependency(&mut self, dependency: Id) {
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
                deps.push(*dep);
                inner(*dep, deps);
            }
        }

        let mut deps = vec![];

        inner(self.id(), &mut deps);

        deps
    }

    pub fn maturity(&self) -> f32 {
        use gkquad::single::integral;

        let result = integral(|x: f64| x, 0.0..1000.).estimate().unwrap();

        result as f32
    }

    pub fn print(&self) -> String {
        self.card.front.clone()
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

    pub fn front_text(&self) -> &str {
        &self.card.front
    }

    #[allow(dead_code)]
    pub fn is_pending(&self) -> bool {
        self.history.is_empty()
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended.is_suspended()
    }

    pub fn is_finished(&self) -> bool {
        self.card.finished
    }

    pub fn set_front_text(&mut self, text: &str) {
        self.card.front = text.to_string();
        self.persist();
    }

    pub fn set_back_text(&mut self, text: &str) {
        self.card.back = text.to_string();
        self.persist();
    }

    pub fn time_since_last_review(&self) -> Option<Duration> {
        self.time_passed_since_last_review()
    }

    pub fn back_text(&self) -> &str {
        &self.card.back
    }

    pub fn id(&self) -> Id {
        self.card.id
    }

    pub fn dependency_ids(&self) -> &BTreeSet<Id> {
        &self.card.dependencies
    }

    pub fn set_finished(&mut self, finished: bool) {
        self.card.finished = finished;
        self.persist();
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

        let toml = toml::to_string(&self.card).unwrap();

        std::fs::write(&path, toml).unwrap();
        *self = SavedCard::from_path(path.as_path())
    }

    pub fn new_review(&mut self, grade: Grade, time: Duration) {
        let review = Review::new(grade, time);
        self.history.add_review(review);
        self.persist();
    }

    pub fn fake_new_review(&mut self, grade: Grade, time: Duration, at_time: Duration) {
        let review = Review {
            timestamp: at_time,
            grade,
            time_spent: time,
        };
        self.history.add_review(review);
    }

    pub fn lapses(&self) -> u32 {
        self.history.lapses()
    }
}

fn is_true(b: &bool) -> bool {
    *b == true
}

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Deserialize, Serialize, Debug, Default, Clone)]
pub struct Card {
    pub front: String,
    pub back: String,
    pub id: Id,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dependencies: BTreeSet<Id>,
    #[serde(default = "default_finished", skip_serializing_if = "is_true")]
    pub finished: bool,
}

impl Matcher for SavedCard {
    fn get_val(&self, key: &str) -> Option<samsvar::Value> {
        match key {
            "front" => json!(&self.front_text()),
            "back" => json!(&self.back_text()),
            "suspended" => json!(&self.is_suspended()),
            "finished" => json!(&self.is_finished()),
            "resolved" => json!(&self.is_resolved()),
            "id" => json!(&self.id().to_string()),
            "recall" => json!(self.recall_rate().unwrap_or_default()),
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

impl Card {
    pub fn import_cards(filename: &Path) -> Option<Vec<Self>> {
        let mut lines = std::io::BufReader::new(std::fs::File::open(filename).ok()?).lines();
        let mut cards = vec![];

        while let Some(Ok(question)) = lines.next() {
            if let Some(Ok(answer)) = lines.next() {
                cards.push(Self::new_simple(question, answer));
            }
        }
        cards.into()
    }

    pub fn new_simple(front: String, back: String) -> Self {
        Card {
            front,
            back,
            id: Uuid::new_v4(),
            finished: true,
            ..Default::default()
        }
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Clone)]
pub enum IsSuspended {
    False,
    True,
    // Card is temporarily suspended, until contained unix time has passed.
    TrueUntil(Duration),
}

impl From<bool> for IsSuspended {
    fn from(value: bool) -> Self {
        match value {
            true => Self::True,
            false => Self::False,
        }
    }
}

impl Default for IsSuspended {
    fn default() -> Self {
        Self::False
    }
}

impl IsSuspended {
    fn verify_time(self) -> Self {
        if let Self::TrueUntil(dur) = self {
            if dur < current_time() {
                return Self::False;
            }
        }
        self
    }

    pub fn is_suspended(&self) -> bool {
        !matches!(self, IsSuspended::False)
    }

    pub fn is_not_suspended(&self) -> bool {
        !self.is_suspended()
    }
}

impl Serialize for IsSuspended {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self.clone().verify_time() {
            IsSuspended::False => serializer.serialize_bool(false),
            IsSuspended::True => serializer.serialize_bool(true),
            IsSuspended::TrueUntil(duration) => serializer.serialize_u64(duration.as_secs()),
        }
    }
}

impl<'de> Deserialize<'de> for IsSuspended {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Value = Deserialize::deserialize(deserializer)?;

        match value {
            Value::Boolean(b) => Ok(b.into()),
            Value::Integer(i) => {
                if let Ok(secs) = std::convert::TryInto::<u64>::try_into(i) {
                    Ok(IsSuspended::TrueUntil(Duration::from_secs(secs)).verify_time())
                } else {
                    Err(de::Error::custom("Invalid duration format"))
                }
            }

            _ => Err(serde::de::Error::custom("Invalid value for IsDisabled")),
        }
    }
}

fn default_finished() -> bool {
    true
}
