/// Various colors used for things.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Colors {
    Blue,
    Green,
    Yellow,
    Red,
    Black,
    White,
}

impl Default for Colors {
    #[tracing::instrument]
    fn default() -> Self {
        Colors::Blue
    }
}

impl ToString for Colors {
    #[tracing::instrument]
    fn to_string(&self) -> String {
        match self {
            Colors::Blue => "#4969F6".to_string(),
            Colors::Green => "#48D597".to_string(),
            Colors::Yellow => "#F5CF65".to_string(),
            Colors::Red => "#E86886".to_string(),
            Colors::Black => "#0B1418".to_string(),
            Colors::White => "#FFFFFF".to_string(),
        }
    }
}
