//! Provider detection: declared dependencies in the nearest package manifest
//! and installed-package lookup through the `node_modules` layout.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::providers::{DOC_PROVIDERS, ProviderDefinition};

const DEPENDENCY_FIELDS: &[&str] =
    &["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"];

/// The nearest `package.json`, walking up from the start directory.
#[derive(Debug)]
pub struct NearestManifest {
    pub dir: PathBuf,
    pub manifest: serde_json::Value,
}

pub fn find_nearest_manifest(start: &Path) -> Option<NearestManifest> {
    let mut dir = start.to_path_buf();
    loop {
        let manifest_path = dir.join("package.json");
        if let Ok(contents) = fs::read_to_string(&manifest_path) {
            // A malformed manifest cannot declare a provider; keep walking up.
            if let Ok(manifest) = serde_json::from_str(&contents) {
                return Some(NearestManifest { dir, manifest });
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Providers whose marker appears in the manifest's declared dependency
/// fields. Detection never selects a provider because a transitive package
/// happens to resolve from `node_modules`.
pub fn detect_providers(manifest: &serde_json::Value) -> Vec<&'static ProviderDefinition> {
    DOC_PROVIDERS
        .iter()
        .filter(|provider| {
            DEPENDENCY_FIELDS.iter().any(|field| {
                manifest
                    .get(field)
                    .and_then(|deps| deps.as_object())
                    .is_some_and(|deps| deps.contains_key(provider.marker))
            })
        })
        .collect()
}

/// An installed package located through the `node_modules` layout.
#[derive(Debug)]
pub struct InstalledPackage {
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
pub fn find_installed_package(name: &str, start: &Path) -> Option<InstalledPackage> {
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
