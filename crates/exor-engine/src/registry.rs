//! Scope validation and metadata-only diffing for the optional registry watch.

use crate::collectors::diff_by_key;
use crate::trie::ChangeType;
use std::collections::BTreeMap;
use std::fmt;

const MAX_SCOPES: usize = 8;
const MAX_INPUT_BYTES: usize = 8_192;

/// Registry root and view. Machine views remain distinct in the evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryHive {
    /// Calling user's Software tree.
    CurrentUser,
    /// Local machine, native 64-bit view.
    Machine,
    /// Local machine, 32-bit view on 64-bit Windows.
    Machine32,
}

/// One explicit subtree under Software, never the entire hive or Software root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryScope {
    hive: RegistryHive,
    subkey: String,
}

impl RegistryScope {
    /// Root and view to open.
    pub fn hive(&self) -> RegistryHive {
        self.hive
    }

    /// Path relative to the hive, beginning with `Software`.
    pub fn subkey(&self) -> &str {
        &self.subkey
    }

    fn parse(text: &str) -> Result<Self, String> {
        let parts: Vec<_> = text.split('\\').collect();
        let invalid = || {
            format!(
                r"Choose a specific registry subtree such as HKCU\Software\Example, not '{text}'."
            )
        };
        let [hive, software, tail @ ..] = parts.as_slice() else {
            return Err(invalid());
        };
        if !software.eq_ignore_ascii_case("software")
            || tail.is_empty()
            || tail.len() > 32
            || tail.iter().any(|part| {
                part.is_empty()
                    || matches!(*part, "." | "..")
                    || part.contains('/')
                    || part.chars().any(char::is_control)
                    || part.encode_utf16().count() > 255
            })
        {
            return Err(invalid());
        }
        let hive = match hive.to_ascii_uppercase().as_str() {
            "HKCU" | "HKEY_CURRENT_USER" => RegistryHive::CurrentUser,
            "HKLM" | "HKEY_LOCAL_MACHINE" => RegistryHive::Machine,
            "HKLM32" => RegistryHive::Machine32,
            _ => return Err(invalid()),
        };
        // Route redirected machine keys through Windows' separate view, not an alias path.
        if hive != RegistryHive::CurrentUser && tail[0].eq_ignore_ascii_case("WOW6432Node") {
            return Err(r"Use HKLM32\Software\… to watch the 32-bit machine view.".into());
        }
        Ok(Self {
            hive,
            subkey: format!(r"Software\{}", tail.join("\\")),
        })
    }
}

impl fmt::Display for RegistryScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let root = match self.hive {
            RegistryHive::CurrentUser => "HKCU",
            RegistryHive::Machine => "HKLM",
            RegistryHive::Machine32 => "HKLM32",
        };
        write!(f, r"{root}\{}", self.subkey)
    }
}

/// Validated set of non-overlapping registry subtrees; empty means opt out.
#[derive(Debug, Default)]
pub struct RegistryScopes(Vec<RegistryScope>);

impl RegistryScopes {
    /// Parse one path per line. Blank lines and scopes already covered by a parent are ignored.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_INPUT_BYTES {
            return Err("The registry scope list is too long.".into());
        }
        let mut sorted = BTreeMap::new();
        for line in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
            let scope = RegistryScope::parse(line)?;
            sorted
                .entry(scope.to_string().to_ascii_lowercase())
                .or_insert(scope);
        }
        // Case-folded parent paths sort before their descendants. Check the separator to avoid
        // treating e.g. ExampleTools as part of Example.
        let mut scopes: Vec<(String, RegistryScope)> = Vec::new();
        for (path, scope) in sorted {
            if !scopes.iter().any(|(parent, _)| {
                path.strip_prefix(parent)
                    .is_some_and(|rest| rest.starts_with('\\'))
            }) {
                scopes.push((path, scope));
                if scopes.len() > MAX_SCOPES {
                    return Err(format!("Choose at most {MAX_SCOPES} registry subtrees."));
                }
            }
        }
        Ok(Self(scopes.into_iter().map(|(_, scope)| scope).collect()))
    }

    /// Normalized, disjoint scopes.
    pub fn as_slice(&self) -> &[RegistryScope] {
        &self.0
    }
}

/// Key metadata used only for comparison; no registry value content is collected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryKeyState {
    /// Fully qualified key path, including the registry view.
    pub path: String,
    /// Windows key last-write stamp.
    pub last_write: u64,
}

/// Change observed between two snapshots of a registry subtree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryChange {
    /// Key path (the older spelling for a removal).
    pub path: String,
    /// Added, modified, or removed.
    pub change: ChangeType,
}

/// Compare by key path (ignoring ASCII case) and last-write stamp, not individual values.
pub fn diff_registry_keys(
    old: &[RegistryKeyState],
    new: &[RegistryKeyState],
) -> Vec<RegistryChange> {
    diff_by_key(
        old,
        new,
        |key| key.path.to_ascii_lowercase(),
        |a, b| a.last_write == b.last_write,
    )
    .into_iter()
    .map(|(key, change)| RegistryChange {
        path: key.path,
        change,
    })
    .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn scopes_are_validated_and_overlaps_collapse_within_the_same_view() {
        let scopes = RegistryScopes::parse(
            &[
                r" HKEY_CURRENT_USER\software\Example\Child ",
                r" HKCU\Software\Example ",
                r"hkcu\software\example",
                r"HKCU\Software\ExampleTools",
                r"HKLM\Software\Example",
                r"HKLM32\Software\Example",
                "",
            ]
            .join("\n"),
        )
        .unwrap();
        assert_eq!(
            scopes
                .as_slice()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            [
                r"HKCU\Software\Example",
                r"HKCU\Software\ExampleTools",
                r"HKLM32\Software\Example",
                r"HKLM\Software\Example",
            ]
        );
        for invalid in [
            r"HKCU\Software",
            r"HKCR\Software\Example",
            r"HKLM\System\Example",
            r"HKCU\Software\\Example",
            r"HKCU\Software\..\Example",
            r"HKLM\Software\WOW6432Node\Example",
            "HKCU\\Software\\Exam\0ple",
        ] {
            assert!(RegistryScopes::parse(invalid).is_err(), "{invalid}");
        }
        let many: Vec<_> = (0..=MAX_SCOPES)
            .map(|i| format!(r"HKCU\Software\App{i}"))
            .collect();
        assert!(RegistryScopes::parse(&many.join("\n")).is_err());
        assert!(RegistryScopes::parse(&" ".repeat(MAX_INPUT_BYTES + 1)).is_err());
        assert!(RegistryScopes::parse("  ").unwrap().as_slice().is_empty());
    }

    #[test]
    fn registry_diff_ignores_spelling_alone_and_classifies_metadata_changes() {
        let key = |name: &str, last_write| RegistryKeyState {
            path: format!(r"HKCU\Software\Example\{name}"),
            last_write,
        };
        let old = [key("Same", 1), key("Edited", 1), key("Removed", 1)];
        let new = [key("same", 1), key("edited", 2), key("Added", 1)];
        let changes = diff_registry_keys(&old, &new);
        assert_eq!(
            changes
                .iter()
                .map(|c| (c.path.rsplit('\\').next().unwrap(), c.change))
                .collect::<Vec<_>>(),
            [
                ("Added", ChangeType::Added),
                ("edited", ChangeType::Modified),
                ("Removed", ChangeType::Removed),
            ]
        );
    }
}
