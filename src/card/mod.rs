use crate::cache;
use crate::categories::Category;
use crate::collections::Collection;
use crate::common::{open_file_with_vim, system_time_as_unix_time};
use crate::concept::{Attribute, Concept};
use crate::concept::{AttributeId, ConceptId};
use crate::reviews::{Recall, Review, Reviews};
use crate::{common::current_time, common::CardId};
use rayon::prelude::*;
use samsvar::json;
use samsvar::Matcher;
use sanitize_filename::sanitize;
use serializing::{RawCard, RawType};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt::{Debug, Display};
use std::fs::{self, create_dir_all, read_to_string};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

pub type RecallRate = f32;

mod serializing;

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
    pub front: String,
    pub back: BackSide,
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
    pub name: String,
    pub concept: ConceptId,
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
    pub attribute: AttributeId,
    pub back: BackSide,
    pub concept_card: CardId,
}

#[derive(Debug, Clone)]
pub struct UnfinishedCard {
    pub front: String,
}

pub trait CardTrait: Debug + Clone {
    fn get_dependencies(&self) -> BTreeSet<CardId>;
    fn display_front(&self) -> String;
    fn generate_new_file_path(&self, category: &Category) -> PathBuf {
        let mut file_name = sanitize(self.display_front().replace(" ", "_").replace("'", ""));
        let dir = category.as_path();
        create_dir_all(&dir).unwrap();

        let mut path = dir.join(&file_name);
        path.set_extension("toml");

        while path.exists() {
            file_name.push_str("_");
            path = dir.join(&file_name);
            path.set_extension("toml");
        }

        path
    }
}

pub trait Reviewable {
    fn display_back(&self) -> String;
}

impl<T: Reviewable + CardTrait> Reviewable for SavedCard<T> {
    fn display_back(&self) -> String {
        self.data.display_back()
    }
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

#[derive(Debug, Clone)]
pub enum AnyType {
    Concept(ConceptCard),
    Normal(NormalCard),
    Unfinished(UnfinishedCard),
    Attribute(AttributeCard),
}

impl AnyType {
    pub fn type_name(&self) -> &str {
        match self {
            AnyType::Concept(_) => "concept",
            AnyType::Normal(_) => "normal",
            AnyType::Unfinished(_) => "unfinished",
            AnyType::Attribute(_) => "attribute",
        }
    }

    pub fn is_concept(&self) -> bool {
        matches!(self, Self::Concept(_))
    }
    pub fn is_finished(&self) -> bool {
        !matches!(self, Self::Unfinished(_))
    }

    pub fn set_backside(self, new_back: BackSide) -> Self {
        match self {
            x @ AnyType::Concept(_) => x,
            AnyType::Normal(NormalCard { front, .. }) => NormalCard {
                front,
                back: new_back,
            }
            .into(),
            AnyType::Unfinished(UnfinishedCard { front }) => NormalCard {
                front,
                back: new_back,
            }
            .into(),
            AnyType::Attribute(AttributeCard {
                attribute,
                concept_card,
                ..
            }) => AttributeCard {
                attribute,
                back: new_back,
                concept_card,
            }
            .into(),
        }
    }
}

impl From<NormalCard> for AnyType {
    fn from(value: NormalCard) -> Self {
        Self::Normal(value)
    }
}
impl From<UnfinishedCard> for AnyType {
    fn from(value: UnfinishedCard) -> Self {
        Self::Unfinished(value)
    }
}
impl From<AttributeCard> for AnyType {
    fn from(value: AttributeCard) -> Self {
        Self::Attribute(value)
    }
}
impl From<ConceptCard> for AnyType {
    fn from(value: ConceptCard) -> Self {
        Self::Concept(value)
    }
}

impl CardTrait for AnyType {
    fn get_dependencies(&self) -> BTreeSet<CardId> {
        match self {
            AnyType::Concept(card) => card.get_dependencies(),
            AnyType::Normal(card) => card.get_dependencies(),
            AnyType::Unfinished(card) => card.get_dependencies(),
            AnyType::Attribute(card) => card.get_dependencies(),
        }
    }

    fn display_front(&self) -> String {
        match self {
            AnyType::Concept(card) => card.display_front(),
            AnyType::Normal(card) => card.display_front(),
            AnyType::Unfinished(card) => card.display_front(),
            AnyType::Attribute(card) => card.display_front(),
        }
    }
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

impl<T: Reviewable + CardTrait> SavedCard<T> {
    pub fn show_backside(&self) -> String {
        self.data.display_back()
    }
}

impl SavedCard<AttributeCard> {
    pub fn new(attr: AttributeCard, category: &Category) -> SavedCard<AnyType> {
        let raw = RawCard::new_attribute(attr);
        raw.save(&category.as_path())
    }
}

impl SavedCard<AnyType> {
    pub fn card_type(&self) -> &AnyType {
        &self.data
    }

    pub fn set_ref(mut self, reff: CardId) -> SavedCard<AnyType> {
        let backside = BackSide::Card(reff);
        self.data = self.data.set_backside(backside);
        self.persist();
        self
    }

    // potentially expensive function!
    pub fn from_id(id: &CardId) -> Option<SavedCard<AnyType>> {
        let path = cache::path_from_id(*id)?;
        Self::from_path(&path).into()
    }

    pub fn is_finished(&self) -> bool {
        self.data.is_finished()
    }

    pub fn is_concept(&self) -> bool {
        self.data.is_concept()
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
        let raw_card = RawCard::from_card(self.clone());
        *self = raw_card.save(&path)
    }

    pub fn from_path(path: &Path) -> SavedCard<AnyType> {
        let content = read_to_string(path).expect("Could not read the TOML file");
        let Ok(raw_card) = toml::from_str::<RawCard>(&content) else {
            dbg!("faild to read card from path: ", path);
            panic!();
        };

        let last_modified = {
            let system_time = std::fs::metadata(path).unwrap().modified().unwrap();
            system_time_as_unix_time(system_time)
        };

        let id = CardId(raw_card.id);

        SavedCard::<AnyType> {
            id,
            data: raw_card.data.into_any(),
            dependencies: raw_card
                .dependencies
                .into_iter()
                .map(|id| CardId(id))
                .collect(),
            tags: raw_card.tags,
            history: Reviews::load(id).unwrap_or_default(),
            location: CardLocation::new(path),
            last_modified,
            suspended: IsSuspended::from(raw_card.suspended),
        }
    }

    pub fn save_at(raw_card: RawCard, path: &Path) -> SavedCard<AnyType> {
        let s: String = toml::to_string_pretty(&raw_card).unwrap();
        let mut file = fs::File::create_new(&path).unwrap();
        file.write_all(&mut s.as_bytes()).unwrap();
        Self::from_path(&path)
    }

    fn get_cards_from_categories(cats: Vec<Category>) -> Vec<SavedCard<AnyType>> {
        cats.into_par_iter()
            .flat_map(|cat| {
                cat.get_containing_card_paths()
                    .into_par_iter()
                    .map(|path| Self::from_path(&path))
                    .collect::<Vec<SavedCard<AnyType>>>()
            })
            .collect()
    }

    pub fn new_normal(unfinished: NormalCard, category: &Category) -> SavedCard<AnyType> {
        let path = unfinished.generate_new_file_path(category);
        let raw_card = RawCard::new(unfinished);
        Self::save_at(raw_card, &path)
    }
    pub fn new_attribute(unfinished: AttributeCard, category: &Category) -> SavedCard<AnyType> {
        let path = unfinished.generate_new_file_path(category);
        let raw_card = RawCard::new(unfinished);
        Self::save_at(raw_card, &path)
    }
    pub fn new_concept(unfinished: ConceptCard, category: &Category) -> SavedCard<AnyType> {
        let path = unfinished.generate_new_file_path(category);
        let raw_card = RawCard::new(unfinished);
        Self::save_at(raw_card, &path)
    }
    pub fn new_unfinished(unfinished: UnfinishedCard, category: &Category) -> SavedCard<AnyType> {
        let path = unfinished.generate_new_file_path(category);
        let raw_card = RawCard::new(unfinished);
        Self::save_at(raw_card, &path)
    }

    pub fn load_all_cards() -> Vec<SavedCard<AnyType>> {
        let collections = Collection::load_all();

        let mut categories: Vec<Category> = collections
            .into_par_iter()
            .flat_map(|col| col.load_categories())
            .collect();

        let extra_categories = Category::load_all(None);
        categories.extend(extra_categories);

        Self::get_cards_from_categories(categories)
    }

    pub fn load_pending(filter: Option<String>) -> Vec<CardId> {
        Self::load_all_cards()
            .into_par_iter()
            .filter(|card| card.history().is_empty())
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
            .filter(|card| !card.history().is_empty())
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

    pub fn rm_dependency(&mut self, dependency: CardId) -> bool {
        let res = self.dependencies.remove(&dependency);
        self.persist();
        res
    }

    pub fn set_dependency(&mut self, dependency: CardId) {
        if self.id() == dependency {
            return;
        }
        self.dependencies.insert(dependency);
        self.persist();
        cache::add_dependent(dependency, self.id());
    }

    pub fn edit_with_vim(&self) -> SavedCard<AnyType> {
        let path = self.as_path();
        open_file_with_vim(path.as_path()).unwrap();
        Self::from_path(path.as_path())
    }

    pub fn new_review(&mut self, grade: Recall, time: Duration) {
        let review = Review::new(grade, time);
        self.history.add_review(review);
        self.persist();
    }

    pub fn back_side(&self) -> Option<&BackSide> {
        match self.card_type() {
            AnyType::Normal(card) => Some(&card.back),
            AnyType::Concept(_) => None?,
            AnyType::Attribute(card) => Some(&card.back),
            AnyType::Unfinished(_) => None?,
        }
    }

    fn into_type(self, data: impl Into<AnyType>) -> SavedCard<AnyType> {
        let path = self.as_path();
        let mut raw = RawCard::from_card(self);
        raw.data = RawType::from_any(data.into());
        raw.save(&path)
    }

    pub fn into_normal(self, normal: NormalCard) -> SavedCard<AnyType> {
        self.into_type(normal)
    }
    pub fn into_unfinished(self, unfinished: UnfinishedCard) -> SavedCard<AnyType> {
        self.into_type(unfinished)
    }
    pub fn into_attribute(self, attribute: AttributeCard) -> SavedCard<AnyType> {
        self.into_type(attribute)
    }

    pub fn into_concept(self, concept: ConceptCard) -> SavedCard<AnyType> {
        self.into_type(concept)
    }
}

impl<T: CardTrait> SavedCard<T> {
    fn history(&self) -> &Reviews {
        &self.history
    }

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
        self.data.display_front()
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

    pub fn time_since_last_review(&self) -> Option<Duration> {
        self.time_passed_since_last_review()
    }

    pub fn id(&self) -> CardId {
        self.id
    }

    pub fn dependency_ids(&self) -> BTreeSet<CardId> {
        let mut deps = self.dependencies.clone();
        deps.extend(self.data.get_dependencies());
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
            Ordering::Greater => panic!(
            "Card in-memory shouldn't have a last_modified more recent than its corresponding file"
        ),
        }
    }

    pub fn lapses(&self) -> u32 {
        self.history.lapses()
    }
}

impl Matcher for SavedCard<AnyType> {
    fn get_val(&self, key: &str) -> Option<samsvar::Value> {
        match key {
            "front" => json!(&self.data.display_front()),
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
