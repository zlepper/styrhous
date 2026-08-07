#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ApiResource {
    pub group: String,
    pub version: String,
    pub kind: String,
    pub name: String,
}
