pub mod cache;
pub mod card;
pub mod categories;
pub mod common;
pub mod config;
pub mod paths;
pub mod recall_rate;

use card::{Grade, SavedCard};
use common::Id;
use samsvar::Matcher;

pub fn load_cards() -> Vec<Id> {
    SavedCard::load_all_cards()
        .iter()
        .map(|card| card.id())
        .collect()
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

pub fn review(card_id: Id, grade: Grade) {
    let mut card = SavedCard::from_id(&card_id).unwrap();
    card.new_review(grade, Default::default());
}
