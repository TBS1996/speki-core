pub mod cache;
pub mod card;
pub mod categories;
pub mod collections;
pub mod common;
pub mod config;
pub mod github;
pub mod paths;
pub mod recall_rate;
pub mod reviews;

pub use card::SavedCard;
use common::Id;
use reviews::Recall;
use samsvar::Matcher;

pub fn load_cards() -> Vec<Id> {
    SavedCard::load_all_cards()
        .iter()
        .map(|card| card.id())
        .collect()
}

pub fn set_back_text(id: Id, s: String) {
    let mut card = SavedCard::from_id(&id).unwrap();
    card.set_back_text(&s);
}

pub fn set_finished(id: Id, finished: bool) {
    let mut card = SavedCard::from_id(&id).unwrap();
    card.set_finished(finished);
}

pub fn cards_filtered(filter: String) -> Vec<Id> {
    let mut cards = SavedCard::load_all_cards();
    cards.retain(|card| card.clone().eval(filter.clone()));
    cards.iter().map(|card| card.id()).collect()
}

pub fn add_card(front: String, back: String) -> Id {
    let card = card::Card::new_simple(front, back);
    SavedCard::new(card).id()
}

pub fn add_unfinished(front: String) -> Id {
    let mut card = card::Card::new_simple(front, "".to_string());
    card.finished = false;
    SavedCard::new(card).id()
}

pub fn review(card_id: Id, grade: Recall) {
    let mut card = SavedCard::from_id(&card_id).unwrap();
    card.new_review(grade, Default::default());
}

pub fn set_dependency(card_id: Id, dependency: Id) {
    let mut card = SavedCard::from_id(&card_id).unwrap();
    card.set_dependency(dependency);
    cache::add_dependent(dependency, card_id);
}

pub fn card_from_id(card_id: Id) -> SavedCard {
    SavedCard::from_id(&card_id).unwrap()
}

pub fn delete(card_id: Id) {
    let path = SavedCard::from_id(&card_id).unwrap().as_path();
    std::fs::remove_file(path).unwrap();
}

pub fn as_graph() -> String {
    // mermaid::export()
    graphviz::export()
}

pub fn edit(card_id: Id) {
    SavedCard::from_id(&card_id).unwrap().edit_with_vim();
}

// Delete dependencies where card isnt found
pub fn prune_dependencies() {
    for mut card in SavedCard::load_all_cards() {
        let mut rm = vec![];
        for dep in card.dependency_ids() {
            if SavedCard::from_id(&dep).is_none() {
                rm.push(*dep);
            }
        }

        for dep in rm {
            card.rm_dependency(dep);
        }
    }
}

mod mermaid {
    use common::truncate_string;

    use super::*;

    pub fn _export() -> String {
        let mut mermaid = String::from("graph TD;\n");

        for card in SavedCard::load_all_cards() {
            let label = card
                .front_text()
                .to_string()
                .replace(")", "")
                .replace("(", "");
            let label = truncate_string(label, 25);

            mermaid.push_str(&format!("    {}[{}];\n", card.id(), label));

            for &child_id in card.dependency_ids() {
                mermaid.push_str(&format!("    {} --> {};\n", card.id(), child_id));
            }
        }

        mermaid
    }
}

mod graphviz {
    use super::*;

    pub fn export() -> String {
        let mut dot = String::from("digraph G {\nranksep=2.0;\nrankdir=BT;\n");

        for card in SavedCard::load_all_cards() {
            let label = card
                .front_text()
                .to_string()
                .replace(")", "")
                .replace("(", "");

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
            for &child_id in card.dependency_ids() {
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
