use crate::common::current_time;
use crate::common::{serde_duration_as_float_secs, serde_duration_as_secs};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize, Serializer};
use std::time::Duration;

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
    pub fn new(grade: Grade, time_spent: Duration) -> Self {
        Self {
            timestamp: current_time(),
            grade,
            time_spent,
        }
    }

    pub fn time_passed(&self) -> Duration {
        let unix = self.timestamp;
        let current_unix = current_time();
        current_unix.checked_sub(unix).unwrap_or_default()
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
