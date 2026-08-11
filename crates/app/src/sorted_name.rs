use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

#[derive(Eq, PartialEq, Debug)]
pub struct SortedName {
    value: String,
    sort_value: String,
}

impl SortedName {
    pub fn new(value: &str) -> Self {
        SortedName {
            sort_value: value.to_lowercase(),
            value: value.to_owned(),
        }
    }
}

impl From<String> for SortedName {
    fn from(value: String) -> Self {
        Self::new(value.as_str())
    }
}
impl From<&String> for SortedName {
    fn from(value: &String) -> Self {
        Self::new(value.as_str())
    }
}

impl From<&str> for SortedName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl Ord for SortedName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_value
            .cmp(&other.sort_value)
            .then_with(|| self.value.cmp(&other.value))
    }
}

impl PartialOrd for SortedName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for SortedName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.value, f)
    }
}

impl Deref for SortedName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl PartialEq<String> for SortedName {
    fn eq(&self, other: &String) -> bool {
        self.value == *other
    }
}

impl PartialEq<str> for SortedName {
    fn eq(&self, other: &str) -> bool {
        self.value == other
    }
}
