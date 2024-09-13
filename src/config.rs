use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{common::open_file_with_vim, paths::get_share_path};

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct Config {
    pub play_audio: bool,
    pub show_images: bool,
    pub download_media: bool,
    #[serde(
        serialize_with = "option_string_to_empty_string",
        deserialize_with = "empty_string_to_option"
    )]
    pub git_remote: Option<String>,
    #[serde(
        serialize_with = "option_string_to_empty_string",
        deserialize_with = "empty_string_to_option"
    )]
    pub gpt_key: Option<String>,
}

impl Config {
    fn _config_path() -> PathBuf {
        dirs::home_dir()
            .unwrap()
            .join(".config")
            .join("speki")
            .join("config.toml")
    }

    fn config_path() -> PathBuf {
        get_share_path().join("config.toml")
    }

    pub fn edit_with_vim() {
        open_file_with_vim(Self::config_path().as_path()).unwrap();
    }

    pub fn read_git_remote(&self) -> &Option<String> {
        &self.git_remote
    }

    // Save the config to a file
    pub fn save(&self) -> std::io::Result<()> {
        let toml = toml::to_string(&self).expect("Failed to serialize config");
        let mut file = File::create(Self::config_path())?;
        file.write_all(toml.as_bytes())?;
        Ok(())
    }

    // Load the config from a file
    pub fn load() -> std::io::Result<Config> {
        let mut file = match File::open(Self::config_path()) {
            Ok(file) => file,
            Err(_) => {
                let _ =
                    std::fs::rename(Self::config_path(), get_share_path().join("invalid_config"));
                Self::default().save()?;
                File::open(Self::config_path())?
            }
        };

        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let config: Config = toml::from_str(&contents).expect("Failed to deserialize config");
        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            play_audio: true,
            show_images: true,
            download_media: true,
            git_remote: None,
            gpt_key: None,
        }
    }
}

fn option_string_to_empty_string<S>(
    value: &Option<String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(v) => serializer.serialize_str(v),
        None => serializer.serialize_str(""),
    }
}

fn empty_string_to_option<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    Ok(if s.is_empty() { None } else { Some(s) })
}
