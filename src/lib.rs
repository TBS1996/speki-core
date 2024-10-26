use attribute::Attribute;
pub use card::Card;
use card::{AnyType, AttributeCard, CardTrait, InstanceCard, NormalCard, UnfinishedCard};
use categories::Category;
use common::CardId;
use eyre::Result;
use reviews::Recall;
use samsvar::Matcher;
use sanitize_filename::sanitize;
use std::{
    collections::BTreeSet,
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
};
use toml::to_string;

pub mod attribute;
pub mod card;
pub mod categories;
pub mod collections;
pub mod common;
pub mod config;
pub mod github;
pub mod paths;
pub mod recall_rate;
pub mod reviews;

#[derive(Default, Ord, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
pub struct TimeStamp {
    millenium: u32,
    century: Option<u32>,
    decade: Option<u32>,
    year: Option<u32>,
    month: Option<u32>,
    day: Option<u32>,
    hour: Option<u32>,
    minute: Option<u32>,
    after_christ: bool,
}

impl Display for TimeStamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

impl TimeStamp {
    fn display(&self) -> String {
        let era = if self.after_christ { "AD" } else { "BC" };

        let cty = match self.century {
            Some(c) => self.millenium * 10 + c,
            None => {
                let num = self.millenium + 1;
                return format!("{}{} millenium {}", num, Self::suffix(num), era);
            }
        };

        let decade = match self.decade {
            Some(d) => cty * 10 + d,
            None => {
                let num = cty + 1;
                return format!("{}{} century {}", num, Self::suffix(num), era);
            }
        };

        // if after year 1000, AD is implied
        let era = if decade > 100 && self.after_christ {
            ""
        } else {
            era
        };

        let year = match self.year {
            Some(y) => decade * 10 + y,
            None => {
                return format!("{}0s {}", decade, era);
            }
        };

        let month = match self.month {
            Some(m) => m,
            None => {
                return format!("{} {}", year, era);
            }
        };

        let day = match self.day {
            Some(d) => d,
            None => {
                return format!("{} {} {}", Self::month_str(month), year, era);
            }
        };

        let hour = match self.hour {
            Some(h) => h,
            None => {
                return format!("{} {} {} {}", day, Self::month_str(month), year, era);
            }
        };

        match self.minute {
            Some(minute) => {
                format!(
                    "{:02}:{:02} {} {} {} {}",
                    hour,
                    minute,
                    day,
                    Self::month_str(month),
                    year,
                    era
                )
            }
            None => {
                format!(
                    "{} o' clock, {} {} {} {}",
                    hour,
                    day,
                    Self::month_str(month),
                    year,
                    era
                )
            }
        }
    }

    fn month_str(m: u32) -> &'static str {
        match m {
            1 => "jan",
            2 => "feb",
            3 => "mar",
            4 => "apr",
            5 => "may",
            6 => "jun",
            7 => "jul",
            8 => "aug",
            9 => "sep",
            10 => "okt",
            11 => "nov",
            12 => "dec",
            _ => "INVALID MONTH",
        }
    }

    fn suffix(num: u32) -> &'static str {
        match num {
            0 => "th",
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    }

    fn parse_two_digits(iter: &mut impl Iterator<Item = char>) -> Option<u32> {
        let (d1, d2) = (iter.next()?, iter.next()?);
        Some(d1.to_string().parse::<u32>().ok()? * 10 + d2.to_string().parse::<u32>().ok()?)
    }

    pub fn serialize(&self) -> String {
        let mut s = String::new();
        if !self.after_christ {
            s.push('-');
        }

        s.push_str(&self.millenium.to_string());
        s.push_str(
            &self
                .century
                .map(|c| c.to_string())
                .unwrap_or("*".to_string()),
        );
        s.push_str(
            &self
                .decade
                .map(|c| c.to_string())
                .unwrap_or("*".to_string()),
        );
        s.push_str(&self.year.map(|c| c.to_string()).unwrap_or("*".to_string()));

        if let Some(month) = self.month {
            s.push_str(&format!("-{:02}", month));
        } else {
            return s;
        };

        if let Some(day) = self.day {
            s.push_str(&format!("-{:02}", day));
        } else {
            return s;
        };

        if let Some(hour) = self.hour {
            s.push_str(&format!(" {:02}", hour));
        } else {
            return s;
        };

        if let Some(minute) = self.minute {
            s.push_str(&format!(":{:02}", minute));
        }

        s
    }

    pub fn from_string(s: String) -> Option<Self> {
        let mut selv = Self::default();
        let mut s: Vec<char> = s.chars().collect();
        let first = s.first()?;
        if first != &'+' && first != &'-' {
            s.insert(0, '+');
        }

        let mut iter = s.into_iter();

        match iter.next()? {
            '+' => selv.after_christ = true,
            '-' => selv.after_christ = false,
            _ => panic!(),
        }

        selv.millenium = iter.next()?.to_string().parse().ok()?;

        selv.century = match iter.next()? {
            '*' => None,
            num => Some(num.to_string().parse().ok()?),
        };

        selv.decade = match iter.next()? {
            '*' => None,
            num => Some(num.to_string().parse().ok()?),
        };

        selv.year = match iter.next()? {
            '*' => None,
            num => Some(num.to_string().parse().ok()?),
        };

        match iter.next() {
            Some('-') => {}
            Some(' ') => {}
            Some(_) => None?,
            None => return Some(selv),
        }

        selv.month = Some(Self::parse_two_digits(&mut iter)?);

        match iter.next() {
            Some('-') => {}
            Some(' ') => {}
            Some(_) => None?,
            None => return Some(selv),
        }

        selv.day = Some(Self::parse_two_digits(&mut iter)?);

        match iter.next() {
            Some('-') => {}
            Some(' ') => {}
            Some(_) => None?,
            None => return Some(selv),
        }

        selv.hour = Some(Self::parse_two_digits(&mut iter)?);

        match iter.next() {
            Some(':') => {}
            Some(_) => None?,
            None => return Some(selv),
        }

        selv.minute = Some(Self::parse_two_digits(&mut iter)?);

        Some(selv)
    }
}

pub fn load_cards() -> Vec<CardId> {
    Card::load_all_cards()
        .iter()
        .map(|card| card.id())
        .collect()
}

pub fn load_and_persist() {
    for mut card in Card::load_all_cards() {
        card.persist();
    }
}

pub fn get_cached_dependents(id: CardId) -> BTreeSet<CardId> {
    Card::<AnyType>::dependents(id)
}

pub fn cards_filtered(filter: String) -> Vec<CardId> {
    let mut cards = Card::load_all_cards();
    cards.retain(|card| card.clone().eval(filter.clone()));
    cards.iter().map(|card| card.id()).collect()
}

pub fn add_card(front: String, back: String, cat: &Category) -> CardId {
    let data = NormalCard {
        front,
        back: back.into(),
    };
    Card::<AnyType>::new_normal(data, cat).id()
}

pub fn add_unfinished(front: String, category: &Category) -> CardId {
    let data = UnfinishedCard { front };
    Card::<AnyType>::new_unfinished(data, category).id()
}

pub fn review(card_id: CardId, grade: Recall) {
    let mut card = Card::from_id(card_id).unwrap();
    card.new_review(grade, Default::default());
}

pub fn set_class(card_id: CardId, class: CardId) -> Result<()> {
    let card = Card::from_id(card_id).unwrap();

    let instance = InstanceCard {
        name: card.card_type().display_front(),
        class,
    };
    card.into_type(instance);
    Ok(())
}

pub fn set_dependency(card_id: CardId, dependency: CardId) {
    if card_id == dependency {
        return;
    }

    let mut card = Card::from_id(card_id).unwrap();
    card.set_dependency(dependency);
    card.persist();
}

pub fn card_from_id(card_id: CardId) -> Card<AnyType> {
    Card::from_id(card_id).unwrap()
}

pub fn delete(card_id: CardId) {
    let path = Card::from_id(card_id).unwrap().as_path();
    std::fs::remove_file(path).unwrap();
}

pub fn as_graph() -> String {
    // mermaid::export()
    graphviz::export()
}

pub fn edit(card_id: CardId) {
    Card::from_id(card_id).unwrap().edit_with_vim();
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
    use std::collections::BTreeSet;

    use super::*;

    pub fn export() -> String {
        let mut dot = String::from("digraph G {\nranksep=2.0;\nrankdir=BT;\n");
        let mut relations = BTreeSet::default();
        let cards = Card::load_all_cards();

        for card in cards {
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
                relations.insert(format!("    \"{}\" -> \"{}\";\n", card.id(), child_id));
            }
        }

        for rel in relations {
            dot.push_str(&rel);
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
    for card in Card::load_all_cards() {
        if let AnyType::Attribute(AttributeCard {
            attribute,
            instance: concept_card,
            ..
        }) = card.card_type()
        {
            if Attribute::load(*attribute).is_none() {
                println!("error loading attribute for: {:?}", &card);
            }

            match Card::from_id(*concept_card) {
                Some(concept_card) => {
                    if !card.card_type().is_class() {
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
