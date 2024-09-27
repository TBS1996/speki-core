use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{categories::Category, paths::get_cards_path};

pub struct Collection {
    name: String,
}

impl Collection {
    pub fn load_categories(&self) -> Vec<Category> {
        vec![]
    }

    pub fn load_all() -> Vec<Self> {
        let mut out = vec![];

        for dir in get_dirs(&get_cards_path()) {
            let name = dir.file_name().unwrap().to_str().unwrap().to_string();
            out.push(Self { name });
        }

        out
    }

    pub fn path(&self) -> PathBuf {
        get_cards_path().join(&self.name)
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub fn get_files(p: &Path) -> Vec<PathBuf> {
    let mut files = vec![];

    for entry in fs::read_dir(&p).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        if ty.is_file() {
            files.push(entry.path());
        }
    }

    files
}

pub fn get_dirs(p: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![];

    for entry in fs::read_dir(&p).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        if ty.is_dir() {
            dirs.push(entry.path());
        }
    }

    dirs
}
