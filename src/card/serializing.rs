use crate::attribute::AttributeId;
use crate::common::CardId;
use crate::paths;
use filecash::FsLoad;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::path::PathBuf;
use std::time::Duration;
use timestamped::TimeStamp;
use toml::Value;
use uuid::Uuid;

use super::{
    AnyType, AttributeCard, BackSide, Card, CardTrait, ClassCard, EventCard, InstanceCard,
    IsSuspended, NormalCard, StatementCard, UnfinishedCard,
};

fn is_false(flag: &bool) -> bool {
    !flag
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct RawType {
    pub front: Option<String>,
    pub back: Option<BackSide>,
    pub name: Option<String>,
    pub class: Option<Uuid>,
    pub instance: Option<Uuid>,
    pub attribute: Option<Uuid>,
    pub statement: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_event: bool,
    pub event: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

impl RawType {
    pub fn into_any(self) -> AnyType {
        if let Some(statement) = self.statement {
            return StatementCard { front: statement }.into();
        }

        if let Some(event) = self.event {
            let start_time = self
                .start_time
                .clone()
                .map(TimeStamp::from_string)
                .flatten()
                .unwrap_or_default();
            let end_time = self
                .start_time
                .clone()
                .map(TimeStamp::from_string)
                .flatten();

            return EventCard {
                front: event,
                start_time,
                end_time,
            }
            .into();
        }

        match (
            self.front,
            self.back,
            self.name,
            self.class,
            self.attribute,
            self.instance,
        ) {
            (None, Some(back), None, None, Some(attribute), Some(instance)) => AttributeCard {
                attribute: AttributeId::verify(&attribute).unwrap(),
                back,
                instance: CardId(instance),
            }
            .into(),
            (Some(front), Some(back), None, None, None, None) => NormalCard { front, back }.into(),
            (None, None, Some(name), Some(class), None, None) => InstanceCard {
                name,
                class: CardId(class),
            }
            .into(),
            (Some(front), None, None, None, None, None) => UnfinishedCard { front }.into(),
            (None, Some(back), Some(name), class, None, None) => ClassCard {
                name,
                back,
                parent_class: class.map(CardId),
                is_event: self.is_event,
            }
            .into(),
            other => {
                panic!("invalid combination of args: {:?}", other);
            }
        }
    }

    pub fn from_any(ty: AnyType) -> Self {
        let mut raw = Self::default();
        match ty {
            AnyType::Instance(InstanceCard { name, class }) => {
                raw.class = Some(class.into_inner());
                raw.name = Some(name);
            }
            AnyType::Normal(NormalCard { front, back }) => {
                raw.front = Some(front);
                raw.back = Some(back);
            }
            AnyType::Unfinished(UnfinishedCard { front }) => {
                raw.front = Some(front);
            }
            AnyType::Attribute(AttributeCard {
                attribute,
                back,
                instance,
            }) => {
                raw.attribute = Some(attribute.into_inner());
                raw.back = Some(back);
                raw.instance = Some(instance.into_inner());
            }
            AnyType::Class(ClassCard {
                name,
                back,
                parent_class,
                is_event,
            }) => {
                raw.name = Some(name);
                raw.back = Some(back);
                raw.class = parent_class.map(CardId::into_inner);
                raw.is_event = is_event;
            }
            AnyType::Statement(StatementCard { front }) => {
                raw.statement = Some(front);
            }
            AnyType::Event(EventCard {
                front,
                start_time,
                end_time,
            }) => {
                raw.event = Some(front);
                raw.start_time = Some(start_time.serialize());
                raw.end_time = end_time.map(|t| t.serialize());
            }
        };

        raw
    }
}

impl FsLoad for RawCard {
    fn id(&self) -> Uuid {
        self.id
    }

    fn type_name() -> String {
        String::from("speki")
    }

    fn save_paths() -> Vec<PathBuf> {
        let p1 = paths::get_cards_path();
        let p2 = paths::get_collections_path();
        vec![p1, p2]
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
            AnyType::Instance(concept) => Self::new_concept(concept),
            AnyType::Normal(normal) => Self::new_normal(normal),
            AnyType::Unfinished(unfinished) => Self::new_unfinished(unfinished),
            AnyType::Attribute(attribute) => Self::new_attribute(attribute),
            AnyType::Class(class) => Self::new_class(class),
            AnyType::Statement(statement) => Self::new_statement(statement),
            AnyType::Event(event) => Self::new_event(event),
        }
    }

    pub fn new_unfinished(unfinished: UnfinishedCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: RawType::from_any(unfinished.into()),
            ..Default::default()
        }
    }

    pub fn new_event(statement: EventCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: RawType::from_any(statement.into()),
            ..Default::default()
        }
    }

    pub fn new_statement(statement: StatementCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: RawType::from_any(statement.into()),
            ..Default::default()
        }
    }

    pub fn new_class(class: ClassCard) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: RawType::from_any(class.into()),
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
    pub fn new_concept(concept: InstanceCard) -> Self {
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
