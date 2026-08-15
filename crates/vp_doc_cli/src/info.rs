//! `vp doc info`: report the resolved provider without starting the tool.

use std::path::Path;

use serde::Serialize;

use crate::{
    config::DocConfigContext,
    detect::find_installed_package,
    error::Error,
    providers::ProviderDefinition,
    resolve::{self, SelectionSource},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocSelectionSource {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<&'static str>,
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

fn build_report(
    provider: &'static ProviderDefinition,
    source: SelectionSource,
    root: &Path,
) -> DocInfoReport {
    // The version gate applies to the marker package; the reported tool is
    // the executable package, which differs for host-bin providers such as
    // Starlight (marker `@astrojs/starlight`, tool `astro`).
    let marker_version = find_installed_package(provider.marker, root)
        .as_ref()
        .and_then(|package| package.version())
        .map(str::to_string);
    let version_supported = marker_version.as_deref().is_some_and(|version| {
        provider.version_range.is_none_or(|range| resolve::version_satisfies(version, range))
    });
    let tool_package = match provider.target {
        crate::providers::ProviderTarget::PackageBin { package_name, .. } => package_name,
        crate::providers::ProviderTarget::BuiltinVite => provider.marker,
    };
    let tool_version = if tool_package == provider.marker {
        marker_version
    } else {
        find_installed_package(tool_package, root)
            .as_ref()
            .and_then(|package| package.version())
            .map(str::to_string)
    };
    let source = match source {
        SelectionSource::Config => DocSelectionSource { kind: "config", marker: None },
        // `info` takes no `--provider`; a flag source cannot occur here.
        SelectionSource::Flag | SelectionSource::Marker => {
            DocSelectionSource { kind: "dependency-marker", marker: Some(provider.marker) }
        }
    };

    DocInfoReport {
        provider: Some(provider.id),
        display_name: Some(provider.display_name),
        source: Some(source),
        target: Some(provider.target.as_str()),
        tool: Some(DocToolInfo {
            package: tool_package,
            version: tool_version,
            supported_range: provider.version_range,
            version_supported,
        }),
        commands: Some(provider.capabilities.iter().map(|action| action.as_str()).collect()),
        candidates: None,
    }
}

/// Build the info report from the effective root. Reads only manifests and
/// the statically extracted config. An invalid `doc.provider` value is a
/// user error, the same as in the lifecycle commands.
pub fn info_report(cwd: &Path, context: Option<&DocConfigContext>) -> Result<DocInfoReport, Error> {
    match resolve::select_provider(None, context, cwd) {
        Ok(resolve::ProviderSelection::Selected { provider, source }) => {
            Ok(build_report(provider, source, cwd))
        }
        Err(Error::UserMessage(message))
            if context.is_some_and(|context| context.config.provider.is_some()) =>
        {
            Err(Error::UserMessage(message))
        }
        Ok(resolve::ProviderSelection::NoProvider) | Err(_) => Ok(DocInfoReport {
            provider: None,
            display_name: None,
            source: None,
            target: None,
            tool: None,
            commands: None,
            candidates: Some(detected_candidates(cwd)),
        }),
    }
}

fn detected_candidates(root: &Path) -> Vec<&'static str> {
    crate::detect::find_nearest_manifest(root)
        .map(|nearest| crate::detect::detect_providers(&nearest.manifest))
        .unwrap_or_default()
        .iter()
        .map(|provider| provider.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn split_provider_reports_the_executable_as_the_tool() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "@astrojs/starlight": "^0.41.0" } }"#,
        )
        .unwrap();
        let marker_root = dir.path().join("node_modules/@astrojs/starlight");
        fs::create_dir_all(&marker_root).unwrap();
        fs::write(
            marker_root.join("package.json"),
            r#"{ "name": "@astrojs/starlight", "version": "0.41.7" }"#,
        )
        .unwrap();
        let astro_root = dir.path().join("node_modules/astro");
        fs::create_dir_all(&astro_root).unwrap();
        fs::write(
            astro_root.join("package.json"),
            r#"{ "name": "astro", "version": "7.2.2", "bin": { "astro": "astro.js" } }"#,
        )
        .unwrap();
        let report = info_report(dir.path(), None).unwrap();
        assert_eq!(report.provider, Some("starlight"));
        let tool = report.tool.expect("tool info");
        assert_eq!(tool.package, "astro");
        assert_eq!(tool.version.as_deref(), Some("7.2.2"));
        // The version gate stays on the marker package.
        assert!(tool.version_supported);
        let source = report.source.expect("source");
        assert_eq!(source.marker, Some("@astrojs/starlight"));
    }
}
