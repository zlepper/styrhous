use crate::minimal_namespace::MinimalNamespace;
use crate::sorted_name::SortedName;
use itertools::Either;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The namespaces a cluster workspace shows.
///
/// `All` follows the discovered namespaces, so namespaces created while it is
/// selected are included without changing the selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SelectedNamespaces {
    All,
    Selected(BTreeSet<String>),
}

impl Default for SelectedNamespaces {
    fn default() -> Self {
        Self::Selected(BTreeSet::new())
    }
}

impl FromIterator<String> for SelectedNamespaces {
    fn from_iter<I: IntoIterator<Item = String>>(namespaces: I) -> Self {
        Self::Selected(namespaces.into_iter().collect())
    }
}

impl<const N: usize> From<[&str; N]> for SelectedNamespaces {
    fn from(namespaces: [&str; N]) -> Self {
        namespaces.map(str::to_owned).into_iter().collect()
    }
}

impl SelectedNamespaces {
    /// Flip one namespace. Leaving `All` keeps every other discovered namespace.
    pub(crate) fn toggle(
        &mut self,
        namespace: &str,
        discovered: &BTreeMap<SortedName, MinimalNamespace>,
    ) {
        match self {
            Self::All => {
                *self = discovered
                    .values()
                    .map(|candidate| candidate.name.clone())
                    .filter(|candidate| candidate != namespace)
                    .collect();
            }
            Self::Selected(namespaces) => {
                if !namespaces.remove(namespace) {
                    namespaces.insert(namespace.to_owned());
                }
            }
        }
    }

    /// Whether no namespace is selected, as opposed to "All namespaces".
    pub(crate) fn is_nothing_selected(&self) -> bool {
        matches!(self, Self::Selected(namespaces) if namespaces.is_empty())
    }

    /// Forget a namespace that no longer exists. `All` is unaffected.
    pub(crate) fn remove(&mut self, namespace: &str) {
        if let Self::Selected(namespaces) = self {
            namespaces.remove(namespace);
        }
    }

    /// Drop explicitly selected namespaces that were not discovered. `All` is unaffected.
    pub(crate) fn retain_discovered(
        &mut self,
        discovered: &BTreeMap<SortedName, MinimalNamespace>,
    ) {
        if let Self::Selected(namespaces) = self {
            namespaces.retain(|namespace| discovered.contains_key(&SortedName::new(namespace)));
        }
    }
}

/// A selection resolved against the discovered namespaces.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NamespaceScope<'a> {
    selection: &'a SelectedNamespaces,
    discovered: &'a BTreeMap<SortedName, MinimalNamespace>,
}

impl<'a> NamespaceScope<'a> {
    pub(crate) fn new(
        selection: &'a SelectedNamespaces,
        discovered: &'a BTreeMap<SortedName, MinimalNamespace>,
    ) -> Self {
        Self {
            selection,
            discovered,
        }
    }

    pub(crate) fn iter(self) -> impl Iterator<Item = &'a String> + 'a {
        match self.selection {
            SelectedNamespaces::All => {
                Either::Left(self.discovered.values().map(|namespace| &namespace.name))
            }
            SelectedNamespaces::Selected(namespaces) => Either::Right(namespaces.iter()),
        }
    }

    pub(crate) fn contains(self, namespace: &str) -> bool {
        match self.selection {
            SelectedNamespaces::All => self.discovered.contains_key(&SortedName::new(namespace)),
            SelectedNamespaces::Selected(namespaces) => namespaces.contains(namespace),
        }
    }

    pub(crate) fn len(self) -> usize {
        match self.selection {
            SelectedNamespaces::All => self.discovered.len(),
            SelectedNamespaces::Selected(namespaces) => namespaces.len(),
        }
    }

    pub(crate) fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Whether every discovered namespace is shown, either through `All` or an
    /// explicit selection that happens to cover them.
    pub(crate) fn covers_all_discovered(self) -> bool {
        !self.discovered.is_empty()
            && match self.selection {
                SelectedNamespaces::All => true,
                SelectedNamespaces::Selected(namespaces) => self
                    .discovered
                    .values()
                    .all(|namespace| namespaces.contains(&namespace.name)),
            }
    }
}
