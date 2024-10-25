use crate::common::CardId;
use crate::concept::{AttributeId, ConceptId};
use dirs::home_dir;
use fsload::FsLoad;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::path::Path;
use std::time::Duration;
use toml::Value;
use uuid::Uuid;

use super::{
    AnyType, AttributeCard, BackSide, Card, CardTrait, ConceptCard, IsSuspended, NormalCard,
    UnfinishedCard,
};

fn is_false(flag: &bool) -> bool {
    !flag
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct RawType {
    pub front: Option<String>,
    pub back: Option<BackSide>,
    pub name: Option<String>,
    pub concept: Option<Uuid>,
    pub concept_card: Option<Uuid>,
    pub attribute: Option<Uuid>,
}

impl RawType {
    pub fn into_any(self) -> AnyType {
        match (
            self.front,
            self.back,
            self.name,
            self.concept,
            self.attribute,
            self.concept_card,
        ) {
            (None, Some(back), None, None, Some(attribute), Some(concept_card)) => AttributeCard {
                attribute: AttributeId::verify(&attribute).unwrap(),
                back,
                concept_card: CardId(concept_card),
            }
            .into(),
            (Some(front), Some(back), None, None, None, None) => NormalCard { front, back }.into(),
            (None, None, Some(name), Some(concept), None, None) => ConceptCard {
                name,
                concept: ConceptId::verify(&concept).unwrap(),
            }
            .into(),
            (Some(front), None, None, None, None, None) => UnfinishedCard { front }.into(),
            other => {
                panic!("invalid combination of args: {:?}", other);
            }
        }
    }

    pub fn from_any(ty: AnyType) -> Self {
        let mut raw = Self::default();
        match ty {
            AnyType::Concept(ty) => {
                let ConceptCard { concept, name } = ty;
                raw.concept = Some(concept.into_inner());
                raw.name = Some(name);
            }
            AnyType::Normal(ty) => {
                let NormalCard { front, back } = ty;
                raw.front = Some(front);
                raw.back = Some(back);
            }
            AnyType::Unfinished(ty) => {
                let UnfinishedCard { front } = ty;
                raw.front = Some(front);
            }
            AnyType::Attribute(ty) => {
                let AttributeCard {
                    attribute,
                    back,
                    concept_card,
                } = ty;
                raw.attribute = Some(attribute.into_inner());
                raw.back = Some(back);
                raw.concept_card = Some(concept_card.into_inner());
            }
        };

        raw
    }
}

impl FsLoad for RawCard {
    fn id(&self) -> Uuid {
        self.id
    }

    fn dir_path() -> std::path::PathBuf {
        let p = home_dir()
            .unwrap()
            .join(".local")
            .join("share")
            .join("speki")
            .join("cards");
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn file_name(&self) -> String {
        self.data.clone().into_any().display_front()
    }

    fn dependencies(&self) -> BTreeSet<Uuid> {
        let mut deps = self.dependencies.clone();
        let other_deps: BTreeSet<Uuid> = self
            .data
            .clone()
            .into_any()
            .get_dependencies()
            .into_iter()
            .map(|id| id.into_inner())
            .collect();
        deps.extend(other_deps.iter());
        deps
    }
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct RawCard {
    pub id: Uuid,
    #[serde(flatten)]
    pub data: RawType,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dependencies: BTreeSet<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub suspended: bool,
}

impl RawCard {
    pub fn new(card: impl Into<AnyType>) -> Self {
        let card: AnyType = card.into();
        match card {
            AnyType::Concept(concept) => Self::new_concept(concept),
            AnyType::Normal(normal) => Self::new_normal(normal),
            AnyType::Unfinished(unfinished) => Self::new_unfinished(unfinished),
            AnyType::Attribute(attribute) => Self::new_attribute(attribute),
        }
    }

    pub fn new_unfinished(unfinished: UnfinishedCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: RawType::from_any(unfinished.into()),
            ..Default::default()
        }
    }

    pub fn new_attribute(attr: AttributeCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: RawType::from_any(attr.into()),
            ..Default::default()
        }
    }
    pub fn new_concept(concept: ConceptCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: RawType::from_any(concept.into()),
            ..Default::default()
        }
    }
    pub fn new_normal(normal: NormalCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: RawType::from_any(normal.into()),
            ..Default::default()
        }
    }

    pub fn from_card(card: Card<AnyType>) -> Self {
        Self {
            id: card.id.into_inner(),
            data: RawType::from_any(card.data),
            dependencies: card
                .dependencies
                .into_iter()
                .map(|id| id.into_inner())
                .collect(),
            tags: card.tags,
            suspended: card.suspended.is_suspended(),
        }
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
