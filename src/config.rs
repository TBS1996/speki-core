use crate::paths::{self, get_share_path};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fs::{create_dir_all, File},
    io::{Read, Write},
    path::PathBuf,
    process::Command,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Repo {
    remote: String,
    name: String,
}

impl Repo {
    pub fn new(remote: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            remote: remote.into(),
            name: name.into(),
        }
    }

    pub fn path(&self) -> PathBuf {
        let path = paths::get_cards_path().join(&self.name);
        create_dir_all(&path).unwrap();
        path
    }

    pub fn exists(&self) -> bool {
        self.path().join(".git").exists()
    }

    pub fn clone(&self) {
        let output = Command::new("git")
            .arg("clone")
            .arg(&self.remote)
            .arg(&self.path())
            .output()
            .expect("Failed to execute git command");

        if output.status.success() {
            println!("cloned successfully");
        } else {
            println!("unsuccesfull clone: {:?}", &output);
        }
    }

    pub fn pull(&self) {
        if !self.exists() {
            self.clone();
        }

        let output = Command::new("git")
            .arg("-C")
            .arg(&self.path())
            .arg("pull")
            .output()
            .expect("Failed to execute git command");

        if !output.status.success() {
            println!("unsuccesfull pull: {:?}", &output);
        }
    }
}

pub struct Repos(Vec<Repo>);

impl Repos {
    pub fn fetch_all(&self) {
        for repo in &self.0 {
            repo.pull();
        }
    }
}

impl Repos {
    pub fn new(config: &Config) -> Self {
        Self(config.repos.clone())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub repos: Vec<Repo>,
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
            repos: vec![Repo::new("git@github.com:TBS1996/spekibase.git", "main")],
        }
    }
}
