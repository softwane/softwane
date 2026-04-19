use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Update<T> {
    Changed(T),
    Unchanged(T),
}

impl<T> Update<T> {
    pub fn get_value(&self) -> &T {
        match self {
            Self::Changed(value) => value,
            Self::Unchanged(value) => value,
        }
    }

    pub fn is_changed(&self) -> bool {
        matches!(self, Self::Changed(_))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_is_changed() {
        let update = Update::Changed(1);
        assert!(update.is_changed());
    }

    #[test]
    fn test_update_is_not_changed() {
        let update = Update::Unchanged(1);
        assert!(!update.is_changed());
    }
}