use crate::{
    git::Repo,
    paths::{self, get_share_path},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub collections: Vec<Repo>,
}

impl Config {
    pub fn config_path() -> PathBuf {
        paths::config_dir().join("config.toml")
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
            collections: vec![Repo::new(
                "https://github.com/TBS1996/spekigraph.git",
                "main",
            )],
        }
    }
}
