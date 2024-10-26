use samsvar::Value;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize};
use timestamped::TimeStamp;

use super::*;

pub enum CardCharacteristic {
    Any,
    Class,
    Instance,
    SubclassOf(CardId),
}

impl CardCharacteristic {
    pub fn card_matches(&self, card: CardId) -> bool {
        let card = Card::from_id(card).unwrap();

        match self {
            CardCharacteristic::Any => true,
            CardCharacteristic::Class => card.is_class(),
            CardCharacteristic::Instance => card.is_instance(),
            CardCharacteristic::SubclassOf(card_id) => {
                card.load_belonging_classes().contains(&card_id)
            }
        }
    }
}

pub enum BackConstraint {
    Time,
    Card(CardCharacteristic),
    List(Vec<CardCharacteristic>),
}

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BackSide {
    Text(String),
    Card(CardId),
    List(Vec<CardId>),
    Time(TimeStamp),
}

impl Default for BackSide {
    fn default() -> Self {
        Self::Text(Default::default())
    }
}

impl From<String> for BackSide {
    fn from(s: String) -> Self {
        if let Ok(uuid) = Uuid::parse_str(&s) {
            BackSide::Card(CardId(uuid))
        } else if let Some(timestamp) = TimeStamp::from_string(s.clone()) {
            BackSide::Time(timestamp)
        } else {
            BackSide::Text(s)
        }
    }
}

impl BackSide {
    pub fn matches_constraint(&self, constraint: BackConstraint) -> bool {
        match (self, constraint) {
            (BackSide::Card(card_id), BackConstraint::Card(card_characteristic)) => {
                card_characteristic.card_matches(*card_id)
            }
            (BackSide::List(cardlist), BackConstraint::List(vec)) => {
                if cardlist.len() != vec.len() {
                    return false;
                }
                cardlist
                    .iter()
                    .zip(vec.iter())
                    .all(|(card, charac)| charac.card_matches(*card))
            }

            (BackSide::Time(_), BackConstraint::Time) => true,

            (_, _) => false,
        }
    }

    pub fn serialize(self) -> String {
        match self {
            BackSide::Text(s) => s,
            BackSide::Card(id) => id.to_string(),
            BackSide::List(list) => serde_json::to_string(&list).unwrap(),
            BackSide::Time(time_stamp) => time_stamp.serialize(),
        }
    }

    pub fn dependencies(&self) -> BTreeSet<CardId> {
        let mut set = BTreeSet::default();
        match self {
            BackSide::Text(_) => {}
            BackSide::Card(card_id) => {
                let _ = set.insert(*card_id);
            }
            BackSide::List(vec) => {
                set.extend(vec.iter());
            }
            BackSide::Time(_) => {}
        }

        set
    }
}

impl Display for BackSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            BackSide::Time(time) => format!("🕒 {}", time),
            BackSide::Text(s) => s.to_owned(),
            BackSide::Card(id) => Card::from_id(*id).unwrap().print(),
            BackSide::List(list) => list
                .iter()
                .map(|id| Card::from_id(*id).unwrap().print())
                .collect::<Vec<String>>()
                .join(", "),
        };

        write!(f, "{}", text)
    }
}

impl<'de> Deserialize<'de> for BackSide {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        match value {
            Value::Array(arr) => {
                let mut ids = Vec::new();
                for item in arr {
                    if let Value::String(ref s) = item {
                        if let Ok(uuid) = Uuid::parse_str(s) {
                            ids.push(CardId(uuid));
                        } else {
                            return Err(serde::de::Error::custom("Invalid UUID in array"));
                        }
                    } else {
                        return Err(serde::de::Error::custom("Expected string in array"));
                    }
                }
                Ok(BackSide::List(ids))
            }
            Value::String(s) => Ok(s.into()),
            _ => Err(serde::de::Error::custom("Expected a string or an array")),
        }
    }
}

impl Serialize for BackSide {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            BackSide::Time(ref t) => serializer.serialize_str(&t.serialize()),
            BackSide::Text(ref s) => serializer.serialize_str(s),
            BackSide::Card(ref id) => serializer.serialize_str(&id.0.to_string()),
            BackSide::List(ref ids) => {
                let mut seq = serializer.serialize_seq(Some(ids.len()))?;
                for id in ids {
                    seq.serialize_element(&id.0.to_string())?;
                }
                seq.end()
            }
        }
    }
}
