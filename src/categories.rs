use crate::collections::Collection;
use crate::paths::{self, get_cards_path};
use std::path::Path;
use std::path::PathBuf;

// Represent the category that a card is in, can be nested
#[derive(Ord, PartialOrd, Eq, Hash, Debug, Clone, PartialEq)]
pub struct Category {
    collection: String,
    dir: Vec<String>,
}

impl Default for Category {
    fn default() -> Self {
        Self {
            collection: Collection::default().name().to_owned(),
            dir: Default::default(),
        }
    }
}

impl Category {
    pub fn join(mut self, s: &str) -> Self {
        self.dir.push(s.to_string());
        self
    }

    /// Represents the top level of a collection
    fn root() -> Self {
        Self::default()
    }

    pub fn joined(&self) -> String {
        self.dir.join("/")
    }

    fn from_dir_path(path: &Path, collection: &Collection) -> Self {
        let paths = paths::get_cards_path().join(collection.name().to_owned());
        let folder = path.strip_prefix(paths).unwrap();

        let components: Vec<String> = Path::new(folder)
            .components()
            .filter_map(|component| component.as_os_str().to_str().map(String::from))
            .collect();

        let categories = Self {
            dir: components,
            collection: collection.name().to_owned(),
        };

        if categories.as_path().exists() {
            categories
        } else {
            panic!();
        }
    }

    pub fn from_card_path(path: &Path) -> Self {
        let path = path.parent().unwrap().to_owned();
        let without_prefix = path.strip_prefix(paths::get_cards_path()).unwrap();
        let mut components = without_prefix.components();
        let col_name = components
            .next()
            .unwrap()
            .as_os_str()
            .to_str()
            .unwrap()
            .to_string();

        let mut dirs = vec![];

        for c in components {
            dirs.push(c.as_os_str().to_string_lossy().to_string());
        }

        Self {
            collection: col_name,
            dir: dirs,
        }
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

    pub fn get_following_categories(&self, collection: &Collection) -> Vec<Self> {
        let categories = Category::load_all(collection);
        let catlen = self.dir.len();
        categories
            .into_iter()
            .filter(|cat| cat.dir.len() >= catlen && cat.dir[0..catlen] == self.dir[0..catlen])
            .collect()
    }

    pub fn print_it(&self) -> String {
        self.dir.last().unwrap_or(&"root".to_string()).clone()
    }

    pub fn print_full(&self) -> String {
        let mut s = "/".to_string();
        s.push_str(&self.joined());
        s
    }

    pub fn print_it_with_depth(&self) -> String {
        let mut s = String::new();
        for _ in 0..self.dir.len() {
            s.push_str("  ");
        }
        format!("{}{}", s, self.print_it())
    }

    fn is_visible_dir(entry: &walkdir::DirEntry) -> bool {
        entry.file_type().is_dir() && !entry.file_name().to_string_lossy().starts_with(".")
    }

    pub fn load_all(collection: &Collection) -> Vec<Self> {
        let mut output = vec![];
        use walkdir::WalkDir;

        for entry in WalkDir::new(collection.path())
            .into_iter()
            .filter_entry(|e| Self::is_visible_dir(e))
            .filter_map(Result::ok)
        {
            let cat = Self::from_dir_path(entry.path(), collection);
            if cat != Self::root() {
                output.push(cat);
            }
        }

        output
    }

    pub fn as_path(&self) -> PathBuf {
        let categories = self.dir.join("/");
        let path = format!(
            "{}/{}",
            get_cards_path().join(&self.collection).to_string_lossy(),
            categories
        );
        PathBuf::from(path)
    }
}

/*
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

*/
