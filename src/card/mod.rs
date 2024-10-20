use crate::categories::Category;
use crate::concept::{Attribute, Concept};
use crate::concept::{AttributeId, ConceptId};
use crate::{common::current_time, common::Id};
use serde::de::Deserializer;
use serde::{de, Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt::Display;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::time::Duration;
use toml::Value;
use uuid::Uuid;

mod saved_card;

pub use saved_card::SavedCard;

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

#[derive(Serialize, Deserialize, Default)]
struct RawCard {
    id: Uuid,
    front: Option<String>,
    back: Option<String>,
    name: Option<String>,
    concept: Option<ConceptId>,
    concept_card: Option<Id>,
    attribute: Option<AttributeId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    dependencies: BTreeSet<Id>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    tags: BTreeMap<String, String>,
}

impl RawCard {
    fn from_card(card: Card) -> Self {
        let mut raw = Self::default();
        raw.id = card.id;

        match card.data {
            CardType::Normal { front, back } => {
                raw.front = Some(front);
                raw.back = Some(back.serialize());
            }
            CardType::Concept { name, concept } => {
                raw.name = Some(name);
                raw.concept = Some(concept);
            }
            CardType::Attribute {
                back,
                attribute,
                concept_card,
            } => {
                raw.back = Some(back.serialize());
                raw.attribute = Some(attribute);
                raw.concept_card = Some(concept_card);
            }
            CardType::Unfinished { front } => {
                raw.front = Some(front);
            }
        };

        raw.dependencies = card.dependencies;
        raw.tags = card.tags;

        raw
    }

    fn into_card(self) -> Option<Card> {
        let mut concept_card = None;
        if self.attribute.is_some() {
            concept_card = if let Some(concept) = self.concept_card {
                Some(concept)
            } else {
                Some(
                    self.dependencies
                        .iter()
                        .find(|id| SavedCard::from_id(id).unwrap().concept().is_some())
                        .copied()
                        .unwrap(),
                )
            };
        };

        let data = match (
            self.front,
            self.back,
            self.name,
            self.concept,
            self.attribute,
        ) {
            (None, Some(back), None, None, Some(attribute)) => CardType::Attribute {
                attribute,
                back: back.into(),
                concept_card: concept_card.unwrap(),
            },
            (Some(front), Some(back), None, None, None) => CardType::Normal {
                front,
                back: back.into(),
            },
            (None, None, Some(name), Some(concept), None) => CardType::Concept { name, concept },
            (Some(front), None, None, None, None) => CardType::Unfinished { front },
            other => {
                println!("invalid combination of args: {:?}", other);
                return None;
            }
        };

        Some(Card {
            data,
            id: self.id,
            dependencies: self.dependencies,
            tags: self.tags,
        })
    }
}

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BackSide {
    Text(String),
    Card(Id),
}

impl Default for BackSide {
    fn default() -> Self {
        Self::Text(Default::default())
    }
}

impl From<String> for BackSide {
    fn from(s: String) -> Self {
        if let Ok(id) = s.parse::<Uuid>() {
            Self::Card(id)
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

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum CardType {
    Normal {
        front: String,
        back: BackSide,
    },
    Concept {
        name: String,
        concept: ConceptId,
    },
    Attribute {
        attribute: AttributeId,
        back: BackSide,
        concept_card: Id,
    },
    Unfinished {
        front: String,
    },
}

impl CardType {
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

    pub fn dependencies(&self) -> BTreeSet<Id> {
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
pub struct Card {
    pub id: Id,
    pub data: CardType,
    pub dependencies: BTreeSet<Id>,
    pub tags: BTreeMap<String, String>,
}

impl Card {
    pub fn card_type(&self) -> &CardType {
        &self.data
    }

    pub fn new(data: CardType) -> Self {
        let dependencies = if let CardType::Attribute { attribute, .. } = &data {
            let concept = Attribute::load(*attribute).unwrap().concept;
            let mut set = BTreeSet::new();
            set.insert(concept);
            set
        } else {
            Default::default()
        };

        Self {
            id: Uuid::new_v4(),
            data,
            dependencies,
            tags: Default::default(),
        }
    }

    pub fn display(&self) -> String {
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
    }

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
        let data = CardType::Normal {
            front,
            back: back.into(),
        };
        Card {
            data,
            id: Uuid::new_v4(),
            dependencies: Default::default(),
            tags: Default::default(),
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
