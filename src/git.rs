use crate::{
    config::Config,
    paths::{self},
};
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};
use serde::{Deserialize, Serialize};
use std::{fs::create_dir_all, path::PathBuf};

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
        if self.exists() {
            println!("Repository already exists at {}", self.path().display());
            return;
        }

        match Repository::clone(&self.remote, &self.path()) {
            Ok(_) => println!("Repository cloned successfully"),
            Err(e) => println!("Failed to clone repository: {}", e),
        }
    }

    pub fn pull(&self) {
        if !self.exists() {
            self.clone();
            return;
        }

        let repo = match Repository::open(&self.path()) {
            Ok(repo) => repo,
            Err(e) => {
                println!("Failed to open repository: {}", e);
                return;
            }
        };

        let mut remote = match repo.find_remote("origin") {
            Ok(remote) => remote,
            Err(_) => {
                println!("Failed to find remote 'origin'.");
                return;
            }
        };

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, _username_from_url, _allowed_types| Cred::default());

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);

        // Fetch latest changes
        if let Err(e) = remote.fetch(&["refs/heads/main"], Some(&mut fetch_options), None) {
            println!("Failed to fetch updates: {}", e);
            return;
        }

        // Merge fetched updates
        let fetch_head = match repo.find_reference("FETCH_HEAD") {
            Ok(fetch_head) => fetch_head,
            Err(e) => {
                println!("Failed to find FETCH_HEAD: {}", e);
                return;
            }
        };

        let commit_id = fetch_head.target().unwrap();
        let commit = repo.find_commit(commit_id).unwrap();

        // Convert commit to AnnotatedCommit
        let annotated_commit = repo.find_annotated_commit(commit.id()).unwrap();
        let (analysis, _) = repo.merge_analysis(&[&annotated_commit]).unwrap();

        if analysis.is_fast_forward() {
            let refname = "refs/heads/main";
            let mut reference = repo.find_reference(refname).unwrap();
            reference.set_target(commit.id(), "Fast-forward").unwrap();
            repo.set_head(refname).unwrap();
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
                .unwrap();
            println!("Fast-forwarded to latest changes.");
        } else {
            println!("Merge required, please resolve manually.");
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

    pub fn new(config: &Config) -> Self {
        Self(config.collections.clone())
    }
}
