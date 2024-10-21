use crate::paths::get_attributes_path;
use crate::SavedCard;
use crate::{
    common::Id, get_containing_file_paths, my_sanitize_filename, paths::get_concepts_path,
};
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use uuid::Uuid;

pub type ConceptId = Uuid;
pub type AttributeId = Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Concept {
    pub name: String,
    pub dependencies: Vec<Id>,
    pub id: ConceptId,
}

impl Concept {
    pub fn load_all() -> Vec<Self> {
        get_containing_file_paths(&get_concepts_path(), None)
            .into_iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    pub fn save(&self) -> Result<()> {
        let filename = my_sanitize_filename(&self.name);
        let path = get_concepts_path().join(filename);
        let mut f = fs::File::create(&path)?;
        let s = toml::to_string_pretty(self)?;
        f.write_all(&mut s.as_bytes())?;

        Ok(())
    }

    pub fn load(id: ConceptId) -> Option<Self> {
        Self::load_all()
            .into_iter()
            .find(|concept| concept.id == id)
    }

    pub fn create(name: String) -> ConceptId {
        let concept = Self {
            name,
            dependencies: vec![],
            id: Uuid::new_v4(),
        };
        concept.save().unwrap();
        concept.id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    pub pattern: String,
    pub id: AttributeId,
    pub concept: ConceptId,
}

impl Attribute {
    pub fn name(&self, card: Id) -> String {
        let card_text = SavedCard::from_id(&card).unwrap().print();
        if self.pattern.contains("{}") {
            self.pattern.replace("{}", &card_text)
        } else {
            format!("{}: {}", &self.pattern, card_text)
        }
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn load_all() -> Vec<Self> {
        get_containing_file_paths(&get_attributes_path(), None)
            .into_iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .map(|s| toml::from_str(&s).unwrap())
            .collect()
    }

    pub fn save(&self) -> Result<()> {
        let filename = my_sanitize_filename(&self.pattern);
        let path = get_attributes_path().join(filename);
        let mut f = fs::File::create(&path)?;
        let s = toml::to_string_pretty(self)?;
        f.write_all(&mut s.as_bytes())?;

        Ok(())
    }

    pub fn load_from_concept(id: ConceptId) -> Vec<Self> {
        let mut attrs = Self::load_all();
        attrs.retain(|attr| attr.concept == id);
        attrs
    }

    pub fn load(id: AttributeId) -> Option<Self> {
        Self::load_all()
            .into_iter()
            .find(|concept| concept.id == id)
    }

    pub fn create(pattern: String, concept: ConceptId) -> AttributeId {
        let attr = Self {
            pattern,
            id: Uuid::new_v4(),
            concept,
        };

        attr.save().unwrap();
        attr.id
    }
}
