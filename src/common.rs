use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};
use uuid::Uuid;

//use crate::paths::get_cards_path;
use std::io::{self, ErrorKind};

use std::time::SystemTime;

use crate::paths::get_review_path;
use crate::SavedCard;

pub type Id = Uuid;

pub fn duration_to_days(dur: &Duration) -> f32 {
    dur.as_secs_f32() / 86400.
}

pub fn days_to_duration(days: f32) -> Duration {
    Duration::from_secs_f32(days * 86400.)
}

pub fn current_time() -> Duration {
    system_time_as_unix_time(SystemTime::now())
}

pub fn system_time_as_unix_time(time: SystemTime) -> Duration {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .expect("Time went backwards")
}

/// Safe way to truncate string.
pub fn truncate_string(input: String, max_len: usize) -> String {
    let mut graphemes = input.chars();
    let mut result = String::new();

    for _ in 0..max_len {
        if let Some(c) = graphemes.next() {
            result.push(c);
        } else {
            break;
        }
    }

    result
}

/// Returns the ids of all cards that have at least one review
///
/// meaning, it has an entry in the reviews folder.
pub fn get_reviewed_cards() -> Vec<Id> {
    let mut cards = vec![];

    for file in std::fs::read_dir(&get_review_path()).unwrap() {
        let file = file.unwrap();
        if file.file_type().as_ref().unwrap().is_dir() {
            continue;
        }

        let path = file.path();
        let name = path.file_name().unwrap();
        let id: Id = name.to_str().unwrap().parse().unwrap();

        // The reviews folder can have ids that no longer refer to a card if it was deleted so we gotta check
        // that it actually exists first
        if SavedCard::from_id(&id).is_some() {
            cards.push(id);
        }
    }

    cards
}

pub fn filename_sanitizer(s: &str) -> String {
    let s = s.replace(" ", "_").replace("'", "");
    sanitize_filename::sanitize(s)
}

pub mod serde_duration_as_float_secs {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let secs = duration.as_secs_f32();
        serializer.serialize_f32(secs)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = f32::deserialize(deserializer)?;
        Ok(Duration::from_secs_f32(secs))
    }
}

pub mod serde_duration_as_secs {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let secs = duration.as_secs();
        serializer.serialize_u64(secs)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

pub fn open_file_with_vim(path: &Path) -> io::Result<()> {
    let status = Command::new("nvim").arg(path).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            ErrorKind::Other,
            "Failed to open file with vim",
        ))
    }
}

pub fn get_last_modified(path: &Path) -> Duration {
    let metadata = std::fs::metadata(path).unwrap();
    let modified_time = metadata.modified().unwrap();
    let secs = modified_time
        .duration_since(UNIX_EPOCH)
        .map(|s| s.as_secs())
        .unwrap();
    Duration::from_secs(secs)
}

pub fn serialize_duration<S>(duration: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let days = duration.as_ref().map(duration_to_days);
    days.serialize(serializer)
}

pub fn deserialize_duration<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    let days = Option::<f32>::deserialize(deserializer)?;
    Ok(days.map(days_to_duration))
}
