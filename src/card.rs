use serde::{de, Deserialize, Serialize, Serializer};
use toml::Value;

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashSet};
use std::ffi::OsString;
use std::fs::{self, read_to_string};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use std::time::Duration;
use uuid::Uuid;

use crate::categories::Category;
use crate::{common::current_time, common::Id};

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
    location: CardLocation,
    last_modified: Duration,
}

impl SavedCard {
    pub fn new(card: Card) -> Self {
        let filename = truncate_string(card.front.clone(), 30);
        let mut path = crate::paths::get_cards_path().join(&filename);
        if path.exists() {
            path = crate::paths::get_cards_path().join(&card.id.to_string());
        };

        let s: String = toml::to_string_pretty(&card).unwrap();

        let mut file = fs::File::create_new(&path).unwrap();

        file.write_all(&mut s.as_bytes()).unwrap();

        Self::from_path(&path)
    }

    pub fn recall_rate(&self) -> Option<RecallRate> {
        crate::recall_rate::recall_rate(&self.card)
    }

    pub fn maturity(&self) -> f32 {
        use gkquad::single::integral;

        let result = integral(|x: f64| x.sin() * (-x * x).exp(), 0.0..1.0)
            .estimate()
            .unwrap();

        result as f32
    }

    pub fn print(&self) -> String {
        self.card.front.clone()
    }

    pub fn set_priority(&mut self, priority: Priority) {
        self.card.priority = priority;
        self.persist();
    }

    pub fn priority(&self) -> &Priority {
        &self.card.priority
    }

    pub fn reviews(&self) -> &Vec<Review> {
        &self.card.history.0
    }

    pub fn raw_reviews(&self) -> &Reviews {
        &self.card.history
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
        self.card.history.is_empty()
    }

    pub fn is_suspended(&self) -> bool {
        self.card.suspended.is_suspended()
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
        self.card.time_passed_since_last_review()
    }

    pub fn back_text(&self) -> &str {
        &self.card.back
    }

    pub fn contains_tag(&self, tag: &str) -> bool {
        self.card.tags.contains(tag)
    }

    pub fn id(&self) -> Id {
        self.card.id
    }

    pub fn dependency_ids(&self) -> &BTreeSet<Id> {
        &self.card.dependencies
    }

    pub fn set_suspended(&mut self, suspended: IsSuspended) {
        self.card.suspended = suspended;
        self.persist();
    }

    pub fn set_finished(&mut self, finished: bool) {
        self.card.finished = finished;
        self.persist();
    }

    pub fn insert_tag(&mut self, tag: String) {
        self.card.tags.insert(tag);
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

    pub fn get_cards_from_category_recursively(category: &Category) -> HashSet<Self> {
        let mut cards = HashSet::new();
        let cats = category.get_following_categories();
        for cat in cats {
            cards.extend(cat.get_containing_cards());
        }
        cards
    }

    pub fn search_in_cards<'a>(
        input: &'a str,
        cards: &'a HashSet<SavedCard>,
        excluded_cards: &'a HashSet<Id>,
    ) -> Vec<&'a SavedCard> {
        cards
            .iter()
            .filter(|card| {
                (card
                    .card
                    .front
                    .to_ascii_lowercase()
                    .contains(&input.to_ascii_lowercase())
                    || card
                        .card
                        .back
                        .to_ascii_lowercase()
                        .contains(&input.to_ascii_lowercase()))
                    && !excluded_cards.contains(&card.id())
            })
            .collect()
    }

    // expensive function!
    pub fn from_id(id: &Id) -> Option<Self> {
        Self::load_all_cards()
            .into_iter()
            .find(|card| &card.card.id == id)
    }

    pub fn load_all_cards() -> HashSet<SavedCard> {
        Self::get_cards_from_category_recursively(&Category::root())
    }

    pub fn edit_with_vim(&self) -> Self {
        let path = self.as_path();
        open_file_with_vim(path.as_path()).unwrap();
        Self::from_path(path.as_path())
    }

    pub fn from_path(path: &Path) -> Self {
        let content = read_to_string(path).expect("Could not read the TOML file");
        let card: Card = toml::from_str(&content).unwrap();
        let location = CardLocation::new(path);

        let last_modified = {
            let system_time = std::fs::metadata(path).unwrap().modified().unwrap();
            system_time_as_unix_time(system_time)
        };

        Self {
            card,
            location,
            last_modified,
        }
    }

    pub fn into_card(self) -> Card {
        self.card
    }

    // Call this function every time SavedCard is mutated.
    fn persist(&mut self) {
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
        self.card.history.add_review(review);
        self.persist();
    }

    pub fn fake_new_review(&mut self, grade: Grade, time: Duration, at_time: Duration) {
        let review = Review {
            timestamp: at_time,
            grade,
            time_spent: time,
        };
        self.card.history.add_review(review);
    }

    pub fn lapses(&self) -> u32 {
        self.card.history.lapses()
    }
}

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Deserialize, Serialize, Debug, Default, Clone)]
pub struct Card {
    pub front: String,
    pub back: String,
    pub id: Id,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dependencies: BTreeSet<Id>,
    #[serde(default)]
    pub suspended: IsSuspended,
    #[serde(default = "default_finished")]
    pub finished: bool,
    #[serde(default, skip_serializing_if = "Priority::is_default")]
    pub priority: Priority,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub tags: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Reviews::is_empty")]
    pub history: Reviews,
}

use samsvar::json;
use samsvar::Matcher;

impl Matcher for SavedCard {
    fn get_val(&self, key: &str) -> Option<samsvar::Value> {
        match key {
            "front" => json!(&self.front_text()),
            "back" => json!(&self.back_text()),
            "suspended" => json!(&self.is_suspended()),
            "finished" => json!(&self.is_finished()),
            "id" => json!(&self.id().to_string()),
            "priority" => json!(self.priority().as_float()),
            "recall" => json!(self.recall_rate().unwrap_or_default()),

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
            suspended: IsSuspended::False,
            ..Default::default()
        }
    }

    fn time_passed_since_last_review(&self) -> Option<Duration> {
        if current_time() < self.history.0.last()?.timestamp {
            return Duration::default().into();
        }

        Some(current_time() - self.history.0.last()?.timestamp)
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Deserialize, Serialize, Debug, Default, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    // No recall, not even when you saw the answer.
    #[default]
    None,
    // No recall, but you remember the answer when you read it.
    Late,
    // Struggled but you got the answer right or somewhat right.
    Some,
    // No hesitation, perfect recall.
    Perfect,
}

impl Grade {
    pub fn get_factor(&self) -> f32 {
        match self {
            Grade::None => 0.1,
            Grade::Late => 0.25,
            Grade::Some => 2.,
            Grade::Perfect => 3.,
        }
        //factor * Self::randomize_factor()
    }
}

impl std::str::FromStr for Grade {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1" => Ok(Self::None),
            "2" => Ok(Self::Late),
            "3" => Ok(Self::Some),
            "4" => Ok(Self::Perfect),
            _ => Err(()),
        }
    }
}

use crate::common::{
    open_file_with_vim, serde_duration_as_float_secs, serde_duration_as_secs,
    system_time_as_unix_time, truncate_string,
};

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Default, Clone)]
pub struct Reviews(pub Vec<Review>);

impl Reviews {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn into_inner(self) -> Vec<Review> {
        self.0
    }

    pub fn from_raw(reviews: Vec<Review>) -> Self {
        Self(reviews)
    }

    pub fn add_review(&mut self, review: Review) {
        self.0.push(review);
    }

    pub fn lapses(&self) -> u32 {
        self.0.iter().fold(0, |lapses, review| match review.grade {
            Grade::None | Grade::Late => lapses + 1,
            Grade::Some | Grade::Perfect => 0,
        })
    }

    pub fn time_since_last_review(&self) -> Option<Duration> {
        self.0.last().map(Review::time_passed)
    }
}

impl Serialize for Reviews {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Reviews {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut reviews = Vec::<Review>::deserialize(deserializer)?;
        reviews.sort_by_key(|review| review.timestamp);
        Ok(Reviews(reviews))
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Deserialize, Clone, Serialize, Debug, Default)]
pub struct Review {
    // When (unix time) did the review take place?
    #[serde(with = "serde_duration_as_secs")]
    pub timestamp: Duration,
    // Recall grade.
    pub grade: Grade,
    // How long you spent before attempting recall.
    #[serde(with = "serde_duration_as_float_secs")]
    pub time_spent: Duration,
}

impl Review {
    fn new(grade: Grade, time_spent: Duration) -> Self {
        Self {
            timestamp: current_time(),
            grade,
            time_spent,
        }
    }

    fn time_passed(&self) -> Duration {
        let unix = self.timestamp;
        let current_unix = current_time();
        current_unix.checked_sub(unix).unwrap_or_default()
    }
}

use serde::de::Deserializer;

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

/// How important a given card is, where 0 is the least important, 100 is most important.
#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Clone)]
pub struct Priority(u32);

impl Priority {
    pub fn as_float(&self) -> f32 {
        self.to_owned().into()
    }

    pub fn is_default(&self) -> bool {
        Self::default() == *self
    }
}

impl TryFrom<char> for Priority {
    type Error = ();

    fn try_from(value: char) -> Result<Self, Self::Error> {
        let pri = match value {
            '1' => 16,
            '2' => 33,
            '3' => 66,
            '4' => 83,
            _ => return Err(()),
        };
        Ok(Self(pri))
    }
}

impl From<u32> for Priority {
    fn from(value: u32) -> Self {
        Self(value.clamp(0, 100))
    }
}

impl Default for Priority {
    fn default() -> Self {
        Self(50)
    }
}

impl From<Priority> for f32 {
    fn from(value: Priority) -> Self {
        value.0 as f32 / 100.
    }
}

impl Serialize for Priority {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Priority {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u32::deserialize(deserializer)?;
        if value > 100 {
            Err(serde::de::Error::custom("Invalid priority value"))
        } else {
            Ok(Priority(value))
        }
    }
}
