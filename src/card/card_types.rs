use super::*;

impl CardTrait for NormalCard {
    fn get_dependencies(&self) -> BTreeSet<CardId> {
        let mut set: BTreeSet<CardId> = Default::default();
        set.extend(self.back.dependencies().iter());
        set
    }

    fn display_front(&self) -> String {
        self.front.clone()
    }
}

#[derive(Debug, Clone)]
pub struct NormalCard {
    pub front: String,
    pub back: BackSide,
}

impl CardTrait for ConceptCard {
    fn get_dependencies(&self) -> BTreeSet<CardId> {
        Concept::load(self.concept).unwrap().dependencies
    }

    fn display_front(&self) -> String {
        self.name.clone()
    }
}

#[derive(Debug, Clone)]
pub struct ConceptCard {
    pub name: String,
    pub concept: ConceptId,
}

impl CardTrait for AttributeCard {
    fn get_dependencies(&self) -> BTreeSet<CardId> {
        let mut dependencies = Attribute::load(self.attribute).unwrap().dependencies;
        dependencies.extend(
            Card::from_id(&self.concept_card)
                .unwrap()
                .dependencies
                .iter(),
        );

        dependencies.extend(self.back.dependencies().iter());

        dependencies
    }

    fn display_front(&self) -> String {
        Attribute::load(self.attribute)
            .unwrap()
            .name(self.concept_card)
    }
}

impl CardTrait for UnfinishedCard {
    fn get_dependencies(&self) -> BTreeSet<CardId> {
        Default::default()
    }

    fn display_front(&self) -> String {
        self.front.clone()
    }
}

#[derive(Debug, Clone)]
pub struct AttributeCard {
    pub attribute: AttributeId,
    pub back: BackSide,
    pub concept_card: CardId,
}

#[derive(Debug, Clone)]
pub struct UnfinishedCard {
    pub front: String,
}

pub trait Reviewable {
    fn display_back(&self) -> String;
}

impl<T: Reviewable + CardTrait> Reviewable for Card<T> {
    fn display_back(&self) -> String {
        self.data.display_back()
    }
}

impl Reviewable for AttributeCard {
    fn display_back(&self) -> String {
        self.back.to_string()
    }
}

impl Reviewable for ConceptCard {
    fn display_back(&self) -> String {
        Concept::load(self.concept).unwrap().name
    }
}

impl Reviewable for NormalCard {
    fn display_back(&self) -> String {
        self.back.to_string()
    }
}

impl From<NormalCard> for AnyType {
    fn from(value: NormalCard) -> Self {
        Self::Normal(value)
    }
}
impl From<UnfinishedCard> for AnyType {
    fn from(value: UnfinishedCard) -> Self {
        Self::Unfinished(value)
    }
}
impl From<AttributeCard> for AnyType {
    fn from(value: AttributeCard) -> Self {
        Self::Attribute(value)
    }
}
impl From<ConceptCard> for AnyType {
    fn from(value: ConceptCard) -> Self {
        Self::Concept(value)
    }
}
