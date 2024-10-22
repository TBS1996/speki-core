use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use std::time::SystemTime;
use std::time::{Duration, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, Ord, Eq, PartialEq, PartialOrd, Copy, Hash)]
#[serde(transparent)]
pub struct CardId(pub Uuid);

impl FromStr for CardId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::from_str(s).map(CardId)
    }
}

impl AsRef<Uuid> for CardId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl CardId {
    pub fn into_inner(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for CardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

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

pub fn filename_sanitizer(s: &str) -> String {
    let s = s.replace(" ", "_").replace("'", "");
    sanitize_filename::sanitize(s)
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
