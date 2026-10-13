//! The entries and pages of Time's collection resources.
use super::{CollectionPage, TimeResource};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeResourceEntry {
    pub uri: ResourceUri,
    pub title: String,
}
impl TimeResourceEntry {
    pub fn new(address: TimeResource, title: String) -> Self {
        Self {
            uri: address.to_uri().expect("typed Time address"),
            title,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeResourcePage<C> {
    pub items: Vec<TimeResourceEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<C>,
}
impl<C> TimeResourcePage<C> {
    pub fn from_page<T>(
        page: CollectionPage<T, C>,
        entry: impl Fn(T) -> TimeResourceEntry,
    ) -> Self {
        Self {
            items: page.items.into_iter().map(entry).collect(),
            next_cursor: page.next_cursor,
        }
    }
}
