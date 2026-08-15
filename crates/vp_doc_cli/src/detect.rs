//! Provider detection: declared dependencies in the nearest package manifest
//! and installed-package lookup through the `node_modules` layout.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::providers::{DOC_PROVIDERS, ProviderDefinition};

// `peerDependencies` is deliberately excluded: a theme or plugin package
// peers on its tool without being a documentation site
// (rfcs/doc-command.md, Provider Selection).
const DEPENDENCY_FIELDS: &[&str] = &["dependencies", "devDependencies"];

/// The nearest `package.json` manifest, walking up from the start
/// directory. The walk stops at the first `package.json` file: detection
/// reads only the nearest manifest, and a malformed one declares nothing
/// (rfcs/doc-command.md, Monorepos). Continuing past it could select a
/// provider from an ancestor package.
pub(crate) fn find_nearest_manifest(start: &Path) -> Option<serde_json::Value> {
    let mut dir = start.to_path_buf();
    loop {
        let manifest_path = dir.join("package.json");
        if manifest_path.is_file() {
            return fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|contents| serde_json::from_str(&contents).ok());
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Providers whose marker satisfies the given declared-dependency check.
/// Callers that already hold parsed dependency maps (for example a workspace
/// package graph) pass a lookup over them, so detection never re-reads a
/// manifest another layer parsed.
pub fn detect_providers_by(declares: impl Fn(&str) -> bool) -> Vec<&'static ProviderDefinition> {
    DOC_PROVIDERS.iter().filter(|provider| declares(provider.marker)).collect()
}

/// Providers whose marker appears in the manifest's declared dependency
/// fields. Detection never selects a provider because a transitive package
/// happens to resolve from `node_modules`.
pub(crate) fn detect_providers(manifest: &serde_json::Value) -> Vec<&'static ProviderDefinition> {
    detect_providers_by(|marker| {
        DEPENDENCY_FIELDS.iter().any(|field| {
            manifest
                .get(field)
                .and_then(|deps| deps.as_object())
                .is_some_and(|deps| deps.contains_key(marker))
        })
    })
}

/// Detection against the `package.json` directly inside `dir` (no walk-up).
/// A missing or malformed manifest declares nothing.
pub fn detect_providers_in_dir(dir: &Path) -> Vec<&'static ProviderDefinition> {
    let Ok(contents) = fs::read_to_string(dir.join("package.json")) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return Vec::new();
    };
    detect_providers(&manifest)
}

/// An installed package located through the `node_modules` layout.
#[derive(Debug)]
pub(crate) struct InstalledPackage {
    pub root: PathBuf,
    pub package_json: serde_json::Value,
}

impl InstalledPackage {
    pub fn version(&self) -> Option<&str> {
        self.package_json.get("version").and_then(|version| version.as_str())
    }
}

/// Locate an installed package by walking `node_modules` directories up from
/// the start directory. This follows the installed layout directly instead
/// of the package's export map.
pub(crate) fn find_installed_package(name: &str, start: &Path) -> Option<InstalledPackage> {
    let mut dir = start.to_path_buf();
    loop {
        let mut root = dir.join("node_modules");
        for segment in name.split('/') {
            root.push(segment);
        }
        let manifest_path = root.join("package.json");
        if let Ok(contents) = fs::read_to_string(&manifest_path) {
            // An unreadable installed manifest counts as not installed.
            if let Ok(package_json) = serde_json::from_str(&contents) {
                return Some(InstalledPackage { root, package_json });
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}
