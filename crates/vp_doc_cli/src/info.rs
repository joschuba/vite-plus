//! `vp doc info`: report the resolved provider without starting the tool.

use std::path::Path;

use serde::Serialize;

use crate::{
    detect::{detect_providers, find_installed_package, find_nearest_manifest},
    providers::ProviderDefinition,
    resolve,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocSelectionSource {
    pub kind: &'static str,
    pub marker: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocToolInfo {
    pub package: &'static str,
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported_range: Option<&'static str>,
    pub version_supported: bool,
}

/// The `vp doc info` report. The JSON form serializes this struct verbatim.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocInfoReport {
    pub provider: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<DocSelectionSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<DocToolInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<&'static str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<&'static str>>,
}

impl DocInfoReport {
    /// The report resolves to a usable provider.
    pub fn resolved(&self) -> bool {
        self.provider.is_some()
    }
}

fn build_report(provider: &'static ProviderDefinition, cwd: &Path) -> DocInfoReport {
    let installed = find_installed_package(provider.marker, cwd);
    let version = installed.as_ref().and_then(|package| package.version()).map(str::to_string);
    let version_supported = version.as_deref().is_some_and(|version| {
        provider.version_range.is_none_or(|range| resolve::version_satisfies(version, range))
    });

    DocInfoReport {
        provider: Some(provider.id),
        display_name: Some(provider.display_name),
        source: Some(DocSelectionSource { kind: "dependency-marker", marker: provider.marker }),
        target: Some(provider.target.as_str()),
        tool: Some(DocToolInfo {
            package: provider.marker,
            version,
            supported_range: provider.version_range,
            version_supported,
        }),
        commands: Some(provider.capabilities.iter().map(|action| action.as_str()).collect()),
        candidates: None,
    }
}

/// Build the info report from the effective root. Reads only manifests.
pub fn info_report(cwd: &Path) -> DocInfoReport {
    let detected = find_nearest_manifest(cwd)
        .map(|nearest| detect_providers(&nearest.manifest))
        .unwrap_or_default();

    match detected.as_slice() {
        [provider] => build_report(provider, cwd),
        detected => DocInfoReport {
            provider: None,
            display_name: None,
            source: None,
            target: None,
            tool: None,
            commands: None,
            candidates: Some(detected.iter().map(|provider| provider.id).collect()),
        },
    }
}
