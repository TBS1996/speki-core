pub mod cache;
pub mod card;
pub mod categories;
pub mod common;
pub mod config;
pub mod paths;
pub mod recall_rate;
pub mod reviews;

pub use card::SavedCard;
use common::Id;
use config::{Config, Repos};
use paths::get_repo_path;
use reviews::Grade;
use samsvar::Matcher;

pub fn load_cards() -> Vec<Id> {
    SavedCard::load_all_cards()
        .iter()
        .map(|card| card.id())
        .collect()
}

pub fn fetch_repos() {
    let config = Config::load().unwrap();
    dbg!(&config);
    Repos::new(&config).fetch_all();
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
    dbg!("loading filter cards");
    let mut cards = SavedCard::load_all_cards();
    dbg!("all cards loaded");
    cards.retain(|card| card.clone().eval(filter.clone()));
    dbg!("card filtering");
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

pub fn review(card_id: Id, grade: Grade) {
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

use git2::{FetchOptions, Repository};

fn git_pull(repo: Repository) {
    let mut remote = repo.find_remote("origin").unwrap();
    let mut fetch_options = FetchOptions::new();

    // Fetch the latest changes from the remote
    remote
        .fetch(&["main"], Some(&mut fetch_options), None)
        .unwrap();

    // Fast-forward the local branch to match the fetched changes
    let fetch_head = repo.find_reference("FETCH_HEAD").unwrap();
    let annotated_commit = repo.reference_to_annotated_commit(&fetch_head).unwrap();

    let (analysis, _) = repo.merge_analysis(&[&annotated_commit]).unwrap();
    if analysis.is_fast_forward() {
        let mut reference = repo.find_reference("refs/heads/main").unwrap();
        reference
            .set_target(annotated_commit.id(), "Fast-Forward")
            .unwrap();
        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
    }

    println!("Pull successful, repository updated to match remote!");
}

pub fn fetch_repo() {
    let path = get_repo_path();
    let repo = match Repository::open(&path) {
        Ok(repo) => repo,
        Err(_) => match Repository::clone("https://github.com/TBS1996/spekibase", &path) {
            Ok(repo) => repo,
            Err(e) => {
                panic!("{}", e);
            }
        },
    };

    git_pull(repo);
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

            // Enclose card ID in quotes to avoid syntax issues
            dot.push_str(&format!("    \"{}\" [label=\"{}\"];\n", card.id(), label));

            // Create edges for dependencies, also enclosing IDs in quotes
            for &child_id in card.dependency_ids() {
                dot.push_str(&format!("    \"{}\" -> \"{}\";\n", card.id(), child_id));
            }
        }

        dot.push_str("}\n");
        dot
    }
}
