use crate::models::Price;
use std::cmp::Ordering;

impl Ord for Price {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.integral().cmp(&other.integral()) {
            Ordering::Equal => self.fractional().cmp(&other.fractional()),
            other => other,
        }
    }
}

impl PartialOrd for Price {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Price {
    fn eq(&self, other: &Self) -> bool {
        self.integral() == other.integral() && self.fractional() == other.fractional()
    }
}

impl Eq for Price {}
