use crate::categories::Category;
use crate::concept::Attribute;
use crate::concept::{AttributeId, ConceptId};
use crate::{common::current_time, common::Id};
use serde::de::Deserializer;
use serde::{de, Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
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
                raw.back = Some(back);
            }
            CardType::Concept { name, concept } => {
                raw.name = Some(name);
                raw.concept = Some(concept);
            }
            CardType::Attribute {
                front,
                back,
                attribute,
            } => {
                raw.front = front;
                raw.back = Some(back);
                raw.attribute = Some(attribute);
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
        let data = match (
            self.front,
            self.back,
            self.name,
            self.concept,
            self.attribute,
        ) {
            (front, Some(back), None, None, Some(attribute)) => CardType::Attribute {
                front,
                back,
                attribute,
            },
            (None, None, Some(name), Some(concept), None) => CardType::Concept { name, concept },
            (Some(front), Some(back), None, None, None) => CardType::Normal { front, back },
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
pub enum CardType {
    Normal {
        front: String,
        back: String,
    },
    Concept {
        name: String,
        concept: ConceptId,
    },
    Attribute {
        front: Option<String>, // front is generated but can be overriden (maybe later)
        back: String,
        attribute: AttributeId,
    },
    Unfinished {
        front: String,
    },
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
            CardType::Normal { front, .. } => front.clone(),
            CardType::Concept { name, .. } => name.clone(),
            CardType::Attribute {
                front, attribute, ..
            } => match front.clone() {
                Some(front) => front,
                None => {
                    let id = SavedCard::from_id(self.dependencies.iter().next().unwrap())
                        .unwrap()
                        .id();
                    Attribute::load(*attribute).unwrap().name(id)
                }
            },
            CardType::Unfinished { front } => front.clone(),
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
        let data = CardType::Normal { front, back };
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
