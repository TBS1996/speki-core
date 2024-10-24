use samsvar::Value;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize};

use super::*;

#[derive(Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub enum BackSide {
    Text(String),
    Card(CardId),
    List(Vec<CardId>),
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
        } else if let Ok(list) = serde_json::from_str::<Vec<Uuid>>(&s) {
            Self::List(list.into_iter().map(|id| CardId(id)).collect())
        } else {
            Self::Text(s)
        }
    }
}

impl BackSide {
    pub fn serialize(self) -> String {
        match self {
            BackSide::Text(s) => s,
            BackSide::Card(id) => id.to_string(),
            BackSide::List(list) => serde_json::to_string(&list).unwrap(),
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
        }

        set
    }
}

impl Display for BackSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            BackSide::Text(s) => s.to_owned(),
            BackSide::Card(id) => Card::from_id(id).unwrap().print(),
            BackSide::List(list) => list
                .iter()
                .map(|id| Card::from_id(id).unwrap().print())
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
                        // Try parsing each string as a UUID
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
            Value::String(s) => {
                if let Ok(uuid) = Uuid::parse_str(&s) {
                    Ok(BackSide::Card(CardId(uuid)))
                } else {
                    Ok(BackSide::Text(s))
                }
            }
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
