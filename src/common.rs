use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};
use uuid::Uuid;

//use crate::paths::get_cards_path;
use std::io::{self, ErrorKind};

use std::time::SystemTime;

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

/*
fn open_folder_in_explorer(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(path).status()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).status()?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path).status()?;
    }

    Ok(())
}
*/

/// will generate a number between 0 and 100 and check that it's below the given percentage.
/// so if you input '10', then ofc, 10% of the times it will return true as the number will be below 10
pub fn within_percentage(percentage: u32) -> bool {
    rand_int(100) < percentage
}

pub fn rand_int(max: u32) -> u32 {
    let time = current_time();
    (time.as_micros() ^ time.as_nanos() ^ time.as_millis()) as u32 % max
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

pub enum FileDir {
    File(PathBuf),
    Dir(PathBuf),
}

impl FileDir {
    /// Returns the files and folders of the given directory.
    pub fn non_rec(path: PathBuf) -> Vec<Self> {
        let mut vec = Vec::new();
        if !path.is_dir() {
            panic!("damn bro");
        }

        for entry in path.read_dir().unwrap() {
            let entry = entry.unwrap();
            let file_type = entry.file_type().unwrap();

            if entry.file_name().to_str().unwrap().starts_with('_') {
                continue;
            }

            if file_type.is_dir() {
                vec.push(Self::Dir(entry.path()));
            } else if file_type.is_file() {
                vec.push(Self::File(entry.path()));
            };
        }

        vec
    }

    /// Returns the directories within a directory.
    pub fn dirs(path: PathBuf) -> Vec<PathBuf> {
        Self::non_rec(path)
            .into_iter()
            .filter_map(|x| match x {
                FileDir::File(_) => None,
                FileDir::Dir(path) => Some(path),
            })
            .collect()
    }

    /// Returns the files within a directory.
    pub fn files(path: PathBuf) -> Vec<PathBuf> {
        Self::non_rec(path)
            .into_iter()
            .filter_map(|x| match x {
                FileDir::File(path) => Some(path),
                FileDir::Dir(_) => None,
            })
            .collect()
    }
}

pub type Id = Uuid;

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
