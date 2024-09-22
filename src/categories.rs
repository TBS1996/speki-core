use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::paths::{self, get_cards_path};
use std::path::Path;
use std::path::PathBuf;

// Represent the category that a card is in, can be nested
#[derive(Ord, PartialOrd, Eq, Hash, Debug, Clone, Default, PartialEq)]
pub struct Category(pub Vec<String>);

impl Category {
    pub fn root() -> Self {
        Self::default()
    }

    pub fn private() -> Self {
        Self::root().join("personal")
    }

    pub fn join(mut self, s: &str) -> Self {
        self.0.push(s.to_string());
        self
    }

    pub fn joined(&self) -> String {
        self.0.join("/")
    }

    fn from_dir_path(path: &Path) -> Self {
        let paths = paths::get_cards_path();
        let folder = path.strip_prefix(paths).unwrap();

        let components: Vec<String> = Path::new(folder)
            .components()
            .filter_map(|component| component.as_os_str().to_str().map(String::from))
            .collect();

        let categories = Self(components);

        if categories.as_path().exists() {
            categories
        } else {
            panic!();
        }
    }

    pub fn from_card_path(path: &Path) -> Self {
        let without_prefix = path.strip_prefix(paths::get_cards_path()).unwrap();
        let folder = without_prefix.parent().unwrap();
        let x: Vec<String> = folder
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        Self(x)
    }

    pub fn get_containing_card_paths(&self) -> Vec<PathBuf> {
        let directory = self.as_path();
        let mut paths = vec![];

        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml") {
                paths.push(path)
            }
        }
        paths
    }

    pub fn get_following_categories(&self) -> Vec<Self> {
        let categories = Category::load_all();
        let catlen = self.0.len();
        categories
            .into_iter()
            .filter(|cat| cat.0.len() >= catlen && cat.0[0..catlen] == self.0[0..catlen])
            .collect()
    }

    pub fn print_it(&self) -> String {
        self.0.last().unwrap_or(&"root".to_string()).clone()
    }

    pub fn print_full(&self) -> String {
        let mut s = "/".to_string();
        s.push_str(&self.joined());
        s
    }

    pub fn print_it_with_depth(&self) -> String {
        let mut s = String::new();
        for _ in 0..self.0.len() {
            s.push_str("  ");
        }
        format!("{}{}", s, self.print_it())
    }

    fn is_visible_dir(entry: &walkdir::DirEntry) -> bool {
        entry.file_type().is_dir() && !entry.file_name().to_string_lossy().starts_with(".")
    }

    pub fn load_all() -> Vec<Self> {
        let mut output = vec![];
        let root = get_cards_path();
        use walkdir::WalkDir;

        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_entry(|e| Self::is_visible_dir(e))
            .filter_map(Result::ok)
        {
            let cat = Self::from_dir_path(entry.path());
            if cat != Self::root() {
                output.push(cat);
            }
        }

        output
    }

    pub fn as_path(&self) -> PathBuf {
        let categories = self.0.join("/");
        let path = format!("{}/{}", get_cards_path().to_string_lossy(), categories);
        PathBuf::from(path)
    }
}

impl Serialize for Category {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = self.0.join("/");
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for Category {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StringVisitor;

        impl<'de> Visitor<'de> for StringVisitor {
            type Value = Category;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string representing a category")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Category(value.split('/').map(|s| s.to_string()).collect()))
            }
        }

        deserializer.deserialize_str(StringVisitor)
    }
}
