use crate::cache;
use crate::categories::Category;
use crate::collections::Collection;
use crate::common::{open_file_with_vim, system_time_as_unix_time};
use crate::concept::{Attribute, Concept};
use crate::concept::{AttributeId, ConceptId};
use crate::paths;
use crate::reviews::{Recall, Review, Reviews};
use crate::{common::current_time, common::CardId};
use rayon::prelude::*;
use samsvar::json;
use samsvar::Matcher;
use sanitize_filename::sanitize;
use serde::de::Deserializer;
use serde::{de, Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt::{Debug, Display};
use std::fs::{self, create_dir_all, read_to_string};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use toml::Value;
use uuid::Uuid;

mod saved_card;

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

fn is_false(flag: &bool) -> bool {
    !flag
}

#[derive(Serialize, Deserialize, Default, Debug)]
struct RawCard {
    id: Uuid,
    front: Option<String>,
    back: Option<String>,
    name: Option<String>,
    concept: Option<Uuid>,
    concept_card: Option<Uuid>,
    attribute: Option<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    dependencies: BTreeSet<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_false")]
    suspended: bool,
}

impl RawCard {
    fn new_unfinished(unfinished: UnfinishedCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            front: Some(unfinished.front),
            ..Default::default()
        }
    }
    fn new_attribute(attr: AttributeCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            attribute: attr.attribute.into_inner().into(),
            back: attr.back.serialize().into(),
            concept_card: attr.concept_card.into_inner().into(),
            ..Default::default()
        }
    }
    fn new_concept(concept: ConceptCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            concept: concept.concept.into_inner().into(),
            name: concept.name.into(),
            ..Default::default()
        }
    }
    fn new_normal(normal: NormalCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            front: Some(normal.front),
            back: Some(normal.back.serialize()),
            ..Default::default()
        }
    }

    fn from_card(card: AnyCard) -> Self {
        match card {
            AnyCard::Concept(saved_card) => Self {
                id: saved_card.id.into_inner(),
                front: None,
                back: None,
                name: Some(saved_card.data.name),
                concept: saved_card.data.concept.into_inner().into(),
                concept_card: None,
                attribute: None,
                dependencies: saved_card
                    .dependencies
                    .into_iter()
                    .map(CardId::into_inner)
                    .collect(),
                tags: saved_card.tags,
                suspended: saved_card.suspended.is_suspended(),
            },
            AnyCard::Normal(saved_card) => Self {
                id: saved_card.id.into_inner(),
                front: saved_card.data.front.into(),
                back: saved_card.data.back.serialize().into(),
                name: None,
                concept: None,
                concept_card: None,
                attribute: None,
                dependencies: saved_card
                    .dependencies
                    .into_iter()
                    .map(CardId::into_inner)
                    .collect(),
                tags: saved_card.tags,
                suspended: saved_card.suspended.is_suspended(),
            },
            AnyCard::Unfinished(saved_card) => Self {
                id: saved_card.id.into_inner(),
                front: saved_card.data.front.into(),
                back: None,
                name: None,
                concept: None,
                concept_card: None,
                attribute: None,
                dependencies: saved_card
                    .dependencies
                    .into_iter()
                    .map(CardId::into_inner)
                    .collect(),
                tags: saved_card.tags,
                suspended: saved_card.suspended.is_suspended(),
            },
            AnyCard::Attribute(saved_card) => Self {
                id: saved_card.id.into_inner(),
                front: None,
                back: saved_card.data.back.serialize().into(),
                name: None,
                concept: None,
                concept_card: None,
                attribute: None,
                dependencies: saved_card
                    .dependencies
                    .into_iter()
                    .map(CardId::into_inner)
                    .collect(),
                tags: saved_card.tags,
                suspended: saved_card.suspended.is_suspended(),
            },
        }
    }
}

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BackSide {
    Text(String),
    Card(CardId),
}

impl Default for BackSide {
    fn default() -> Self {
        Self::Text(Default::default())
    }
}

impl From<String> for BackSide {
    fn from(s: String) -> Self {
        if let Ok(id) = s.parse::<Uuid>() {
            Self::Card(CardId(id))
        } else {
            Self::Text(s)
        }
    }
}

impl BackSide {
    fn serialize(self) -> String {
        match self {
            BackSide::Text(s) => s,
            BackSide::Card(id) => id.to_string(),
        }
    }
}

impl Display for BackSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            BackSide::Text(s) => s.to_owned(),
            BackSide::Card(id) => SavedCard::from_id(id).unwrap().print(),
        };

        write!(f, "{}", text)
    }
}

impl CardTrait for NormalCard {
    fn get_dependencies(&self) -> BTreeSet<CardId> {
        let mut set: BTreeSet<CardId> = Default::default();
        if let BackSide::Card(id) = &self.back {
            set.insert(*id);
        }
        set
    }

    fn display_front(&self) -> String {
        self.front.clone()
    }
}

#[derive(Debug, Clone)]
pub struct NormalCard {
    front: String,
    back: BackSide,
}

impl CardTrait for ConceptCard {
    fn get_dependencies(&self) -> BTreeSet<CardId> {
        Concept::load(self.concept).unwrap().dependencies
    }

    fn display_front(&self) -> String {
        self.name.clone()
    }
}

#[derive(Debug, Clone)]
pub struct ConceptCard {
    name: String,
    concept: ConceptId,
}

impl CardTrait for AttributeCard {
    fn get_dependencies(&self) -> BTreeSet<CardId> {
        let mut dependencies = Attribute::load(self.attribute).unwrap().dependencies;
        dependencies.extend(
            SavedCard::from_id(&self.concept_card)
                .unwrap()
                .dependencies
                .iter(),
        );
        if let BackSide::Card(id) = &self.back {
            dependencies.insert(*id);
        }

        dependencies
    }

    fn display_front(&self) -> String {
        Attribute::load(self.attribute)
            .unwrap()
            .name(self.concept_card)
    }
}

impl CardTrait for UnfinishedCard {
    fn get_dependencies(&self) -> BTreeSet<CardId> {
        Default::default()
    }

    fn display_front(&self) -> String {
        self.front.clone()
    }
}

#[derive(Debug, Clone)]
pub struct AttributeCard {
    attribute: AttributeId,
    back: BackSide,
    concept_card: CardId,
}

#[derive(Debug, Clone)]
pub struct UnfinishedCard {
    front: String,
}

pub trait CardTrait: Debug + Clone {
    fn get_dependencies(&self) -> BTreeSet<CardId>;
    fn display_front(&self) -> String;
}

pub trait Reviewable {
    fn display_back(&self) -> String;
}

impl Reviewable for AttributeCard {
    fn display_back(&self) -> String {
        self.back.to_string()
    }
}

impl Reviewable for ConceptCard {
    fn display_back(&self) -> String {
        Concept::load(self.concept).unwrap().name
    }
}

impl Reviewable for NormalCard {
    fn display_back(&self) -> String {
        self.back.to_string()
    }
}

/*
impl CardType {
    pub fn new_attribute(attribute: AttributeId, concept_card: CardId, back: BackSide) -> Self {
        let concept = SavedCard::from_id(&concept_card)
            .unwrap()
            .concept()
            .unwrap();

        assert_eq!(
            concept,
            Attribute::load(attribute).unwrap().concept,
            "card concept doesnt match attribute concept"
        );

        Self::Attribute {
            attribute,
            back,
            concept_card,
        }
    }

    pub fn is_normal(&self) -> bool {
        matches!(self, Self::Normal { .. })
    }
    pub fn is_concept(&self) -> bool {
        matches!(self, Self::Concept { .. })
    }
    pub fn is_attribute(&self) -> bool {
        matches!(self, Self::Attribute { .. })
    }
    pub fn is_unfinished(&self) -> bool {
        matches!(self, Self::Unfinished { .. })
    }

    pub fn display(&self) -> &str {
        match self {
            CardType::Normal { .. } => "normal",
            CardType::Concept { .. } => "concept",
            CardType::Attribute { .. } => "attribute",
            CardType::Unfinished { .. } => "unfinished",
        }
    }

    pub fn dependencies(&self) -> BTreeSet<CardId> {
        let mut dependencies = BTreeSet::default();

        match self {
            CardType::Normal { back, .. } => {
                if let BackSide::Card(id) = back {
                    dependencies.insert(*id);
                }
            }
            CardType::Concept { concept, .. } => {
                dependencies.extend(Concept::load(*concept).unwrap().dependencies.iter());
            }
            CardType::Attribute {
                attribute,
                back,
                concept_card,
            } => {
                dependencies.extend(Attribute::load(*attribute).unwrap().dependencies.iter());
                if let BackSide::Card(id) = back {
                    dependencies.insert(*id);
                }

                dependencies.insert(*concept_card);
            }
            CardType::Unfinished { .. } => {}
        };

        dependencies
    }
}

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub struct Card<T: CardTrait> {
    pub id: CardId,
    pub data: T,
    pub dependencies: BTreeSet<CardId>,
    pub tags: BTreeMap<String, String>,
    pub suspended: bool,
}

impl<T: CardTrait> Card<T> {
    pub fn card_type(&self) -> &T {
        &self.data
    }

    pub fn new(data: T) -> Card<T> {
        let dependencies = data.get_dependencies();

        Self {
            id: CardId(Uuid::new_v4()),
            data,
            dependencies,
            tags: Default::default(),
            suspended: false,
        }
    }

    pub fn display(&self) -> String {
        /*
        match &self.data {
            CardType::Unfinished { front } => front.clone(),
            CardType::Normal { front, .. } => front.clone(),
            CardType::Concept { name, .. } => name.clone(),
            CardType::Attribute {
                attribute,
                concept_card,
                ..
            } => Attribute::load(*attribute).unwrap().name(*concept_card),
        }
        */

        self.data.display_front()
    }

    /*
    */

    pub fn new_normal(front: String, back: String) -> Card<NormalCard> {
        let data = NormalCard {
            front,
            back: back.into(),
        };

        Card {
            data,
            id: CardId(Uuid::new_v4()),
            dependencies: Default::default(),
            tags: Default::default(),
            suspended: false,
        }
    }
}
*/

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

pub enum AnyCard {
    Concept(SavedCard<ConceptCard>),
    Normal(SavedCard<NormalCard>),
    Unfinished(SavedCard<UnfinishedCard>),
    Attribute(SavedCard<AttributeCard>),
}

/// Represents a card that has been saved as a toml file, which is basically anywhere in the codebase
/// except for when youre constructing a new card.
/// Don't save this in containers or pass to functions, rather use the Id, and get new instances of SavedCard from the cache.
/// Also, every time you mutate it, call the persist() method.
#[derive(Clone, Ord, PartialOrd, PartialEq, Eq, Hash, Debug)]
pub struct SavedCard<T: CardTrait + ?Sized> {
    id: CardId,
    data: T,
    dependencies: BTreeSet<CardId>,
    tags: BTreeMap<String, String>,
    history: Reviews,
    location: CardLocation,
    last_modified: Duration,
    suspended: IsSuspended,
}

impl<T: CardTrait> std::fmt::Display for SavedCard<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.data.display_front())
    }
}

/// Associated methods
impl<T: CardTrait + ?Sized> SavedCard<T> {
    pub fn create(data: T, category: &Category) -> SavedCard<T> {
        let card = Card::new(data);
        Self::new_at(card, category)
    }

    pub fn import_cards(filename: &Path) -> Option<Vec<CardTrait<NormalCard>>> {
        let mut lines = std::io::BufReader::new(std::fs::File::open(filename).ok()?).lines();
        let mut cards = vec![];

        while let Some(Ok(question)) = lines.next() {
            if let Some(Ok(answer)) = lines.next() {
                cards.push(Self::new_simple(question, answer));
            }
        }
        cards.into()
    }

    pub fn new_at(data: T, category: &Category) -> SavedCard<T> {
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
    pub fn from_id(id: &CardId) -> Option<SavedCard<CardTrait>> {
        let path = cache::path_from_id(*id)?;
        Self::from_path(&path).into()
    }

    pub fn load_pending(filter: Option<String>) -> Vec<CardId> {
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

    pub fn load_non_pending(filter: Option<String>) -> Vec<CardId> {
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

    pub fn from_path(path: &Path) -> AnyCard {
        let content = read_to_string(path).expect("Could not read the TOML file");
        let Ok(raw_card) = toml::from_str::<RawCard>(&content) else {
            dbg!("faild to read card from path: ", path);
            panic!();
        };

        let suspended = IsSuspended::from(raw_card.suspended);
        let location = CardLocation::new(path);

        let last_modified = {
            let system_time = std::fs::metadata(path).unwrap().modified().unwrap();
            system_time_as_unix_time(system_time)
        };

        let history: Reviews = {
            let path = paths::get_review_path().join(raw_card.id.to_string());
            if path.exists() {
                let s = fs::read_to_string(path).unwrap();
                Reviews::from_str(&s)
            } else {
                Default::default()
            }
        };

        let mut concept_card = None;
        if raw_card.attribute.is_some() {
            concept_card = if let Some(concept) = raw_card.concept_card {
                Some(concept)
            } else {
                panic!("missing concept card: {:?}", raw_card);
            };
        };

        match (
            raw_card.front,
            raw_card.back,
            raw_card.name,
            raw_card.concept,
            raw_card.attribute,
        ) {
            (None, Some(back), None, None, Some(attribute)) => {
                let data = AttributeCard {
                    attribute: AttributeId::verify(&attribute).unwrap(),
                    back: back.into(),
                    concept_card: CardId(concept_card.unwrap()),
                };

                let card = SavedCard::<AttributeCard> {
                    id: CardId(raw_card.id),
                    data,
                    dependencies: raw_card
                        .dependencies
                        .into_iter()
                        .map(|id| CardId(id))
                        .collect(),
                    tags: raw_card.tags,
                    history,
                    location,
                    last_modified,
                    suspended,
                };
                return AnyCard::Attribute(card);
            }
            (Some(front), Some(back), None, None, None) => {
                let data = NormalCard {
                    front,
                    back: back.into(),
                };

                let card = SavedCard::<NormalCard> {
                    id: CardId(raw_card.id),
                    data,
                    dependencies: raw_card
                        .dependencies
                        .into_iter()
                        .map(|id| CardId(id))
                        .collect(),
                    tags: raw_card.tags,
                    history,
                    location,
                    last_modified,
                    suspended,
                };
                return AnyCard::Normal(card);
            }
            (None, None, Some(name), Some(concept), None) => {
                let data = ConceptCard {
                    name,
                    concept: ConceptId::verify(&concept).unwrap(),
                };

                let card = SavedCard::<ConceptCard> {
                    id: CardId(raw_card.id),
                    data,
                    dependencies: raw_card
                        .dependencies
                        .into_iter()
                        .map(|id| CardId(id))
                        .collect(),
                    tags: raw_card.tags,
                    history,
                    location,
                    last_modified,
                    suspended,
                };
                return AnyCard::Concept(card);
            }
            (Some(front), None, None, None, None) => {
                let data = UnfinishedCard { front };

                let card = SavedCard::<UnfinishedCard> {
                    id: CardId(raw_card.id),
                    data,
                    dependencies: raw_card
                        .dependencies
                        .into_iter()
                        .map(|id| CardId(id))
                        .collect(),
                    tags: raw_card.tags,
                    history,
                    location,
                    last_modified,
                    suspended,
                };
                return AnyCard::Unfinished(card);
            }
            other => {
                panic!("invalid combination of args: {:?}", other);
            }
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

    fn time_passed_since_last_review(&self) -> Option<Duration> {
        if current_time() < self.history.0.last()?.timestamp {
            return Duration::default().into();
        }

        Some(current_time() - self.history.0.last()?.timestamp)
    }

    pub fn recall_rate_at(&self, current_unix: Duration) -> Option<RecallRate> {
        crate::recall_rate::recall_rate(&self.history, current_unix)
    }
    pub fn recall_rate(&self) -> Option<RecallRate> {
        let now = current_time();
        crate::recall_rate::recall_rate(&self.history, now)
    }

    pub fn rm_dependency(&mut self, dependency: CardId) -> bool {
        let res = self.card.dependencies.remove(&dependency);
        self.persist();
        res
    }

    pub fn set_attribute(&mut self, id: AttributeId, concept_card: CardId) {
        let back = self.back_side().unwrap().to_owned();
        let data = CardType::new_attribute(id, concept_card, back.into());
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

    pub fn set_dependency(&mut self, dependency: CardId) {
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

    fn all_dependencies(&self) -> Vec<CardId> {
        fn inner(id: CardId, deps: &mut Vec<CardId>) {
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

    pub fn id(&self) -> CardId {
        self.card.id
    }

    pub fn dependency_ids(&self) -> BTreeSet<CardId> {
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
