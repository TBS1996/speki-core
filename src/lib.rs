pub mod cache;
pub mod card;
pub mod categories;
pub mod collections;
pub mod common;
pub mod concept;
pub mod config;
pub mod github;
pub mod paths;
pub mod recall_rate;
pub mod reviews;

use std::path::{Path, PathBuf};

pub use card::SavedCard;
use card::{AnyType, AttributeCard, CardTrait, ConceptCard, NormalCard, UnfinishedCard};
use categories::Category;
use common::CardId;
use concept::{Attribute, Concept, ConceptId};
use reviews::Recall;
use samsvar::Matcher;
use sanitize_filename::sanitize;

pub fn load_cards() -> Vec<CardId> {
    SavedCard::load_all_cards()
        .iter()
        .map(|card| card.id())
        .collect()
}

pub fn load_and_persist() {
    for mut card in SavedCard::load_all_cards() {
        card.persist();
    }
}

pub fn get_cached_dependents(id: CardId) -> Vec<CardId> {
    cache::dependents_from_id(id)
}

pub fn cards_filtered(filter: String) -> Vec<CardId> {
    let mut cards = SavedCard::load_all_cards();
    cards.retain(|card| card.clone().eval(filter.clone()));
    cards.iter().map(|card| card.id()).collect()
}

pub fn add_card(front: String, back: String, cat: &Category) -> CardId {
    let data = NormalCard {
        front,
        back: back.into(),
    };
    SavedCard::<AnyType>::new_normal(data, cat).id()
}

pub fn add_unfinished(front: String, category: &Category) -> CardId {
    let data = UnfinishedCard { front };
    SavedCard::<AnyType>::new_unfinished(data, category).id()
}

pub fn review(card_id: CardId, grade: Recall) {
    let mut card = SavedCard::from_id(&card_id).unwrap();
    card.new_review(grade, Default::default());
}

use eyre::Result;

pub fn set_concept(card_id: CardId, concept: ConceptId) -> Result<()> {
    let card = SavedCard::from_id(&card_id).unwrap();
    assert!(Concept::load(concept).is_some(), "concept not found??");

    let concept = ConceptCard {
        name: card.card_type().display_front(),
        concept,
    };
    card.into_concept(concept);
    Ok(())
}

pub fn set_dependency(card_id: CardId, dependency: CardId) {
    if card_id == dependency {
        return;
    }

    let mut card = SavedCard::from_id(&card_id).unwrap();
    card.set_dependency(dependency);
    cache::add_dependent(dependency, card_id);
}

pub fn card_from_id(card_id: CardId) -> SavedCard<AnyType> {
    SavedCard::from_id(&card_id).unwrap()
}

pub fn delete(card_id: CardId) {
    let path = SavedCard::from_id(&card_id).unwrap().as_path();
    std::fs::remove_file(path).unwrap();
}

pub fn as_graph() -> String {
    // mermaid::export()
    graphviz::export()
}

pub fn edit(card_id: CardId) {
    SavedCard::from_id(&card_id).unwrap().edit_with_vim();
}

pub fn get_containing_file_paths(directory: &Path, ext: Option<&str>) -> Vec<PathBuf> {
    let mut paths = vec![];

    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        match ext {
            Some(ext) => {
                if path.extension().and_then(|s| s.to_str()) == Some(ext) {
                    paths.push(path)
                }
            }
            None => paths.push(path),
        }
    }
    paths
}

pub fn my_sanitize_filename(s: &str) -> String {
    sanitize(s.replace(" ", "_").replace("'", ""))
}

mod graphviz {
    use super::*;

    pub fn export() -> String {
        let mut dot = String::from("digraph G {\nranksep=2.0;\nrankdir=BT;\n");

        for card in SavedCard::load_all_cards() {
            let label = card
                .print()
                .to_string()
                .replace(")", "")
                .replace("(", "")
                .replace("\"", "");

            let color = match card.recall_rate() {
                _ if !card.is_finished() => yellow_color(),
                Some(rate) => rate_to_color(rate as f64 * 100.),
                None => cyan_color(),
            };

            match card.recall_rate() {
                Some(rate) => {
                    let recall_rate = rate * 100.;
                    dot.push_str(&format!(
                        "    \"{}\" [label=\"{} ({:.0}%)\", style=filled, fillcolor=\"{}\"];\n",
                        card.id(),
                        label,
                        recall_rate,
                        color
                    ));
                }
                None => {
                    dot.push_str(&format!(
                        "    \"{}\" [label=\"{} \", style=filled, fillcolor=\"{}\"];\n",
                        card.id(),
                        label,
                        color
                    ));
                }
            }

            // Create edges for dependencies, also enclosing IDs in quotes
            for child_id in card.dependency_ids() {
                dot.push_str(&format!("    \"{}\" -> \"{}\";\n", card.id(), child_id));
            }
        }

        dot.push_str("}\n");
        dot
    }

    // Convert recall rate to a color, from red to green
    fn rate_to_color(rate: f64) -> String {
        let red = ((1.0 - rate / 100.0) * 255.0) as u8;
        let green = (rate / 100.0 * 255.0) as u8;
        format!("#{:02X}{:02X}00", red, green) // RGB color in hex
    }

    fn cyan_color() -> String {
        String::from("#00FFFF")
    }

    fn yellow_color() -> String {
        String::from("#FFFF00")
    }
}

pub fn health_check() {
    println!("STARTING HEALTH CHECK");
    verify_attributes();
    println!("HEALTH CHECK OVER");
}

fn verify_attributes() {
    for card in SavedCard::load_all_cards() {
        if let AnyType::Attribute(AttributeCard {
            attribute,
            concept_card,
            ..
        }) = card.card_type()
        {
            if Attribute::load(*attribute).is_none() {
                println!("error loading attribute for: {:?}", &card);
            }

            match SavedCard::from_id(concept_card) {
                Some(concept_card) => {
                    if !card.card_type().is_concept() {
                        println!(
                            "error, cards concept card is not a concept: {:?} -> {:?}",
                            &card, concept_card
                        )
                    }
                }
                None => {
                    println!("error loading concept card for: {}", &card);
                }
            }
        }
    }
}
