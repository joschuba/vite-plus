//! `vp doc info`: report the resolved provider without starting the tool.

use std::path::Path;

use crate::{
    config::DocConfigContext,
    detect::find_installed_package,
    error::Error,
    providers::{ProviderDefinition, ProviderTarget},
    resolve::{self, SelectionSource},
};

/// A report that resolved to a usable provider. The marker and the
/// execution target stay separate subjects, so a consumer can tell which
/// package the version gate checked (rfcs/doc-command.md, Selection
/// reporting).
#[derive(Debug)]
pub struct DocInfoResolved {
    pub provider: &'static ProviderDefinition,
    pub source: SelectionSource,
    /// The installed version of the marker package, the subject of the
    /// version gate.
    pub marker_version: Option<String>,
    /// The installed version of the execution package (`astro` for
    /// Starlight); equals `marker_version` when the two coincide.
    pub execution_version: Option<String>,
    /// The marker version against the provider's tool version range.
    pub supported: bool,
}

impl DocInfoResolved {
    /// The executable package: the bin package for a package-bin target,
    /// the marker itself for the built-in Vite target.
    pub fn execution_package(&self) -> &'static str {
        match self.provider.target {
            ProviderTarget::PackageBin { package_name, .. } => package_name,
            ProviderTarget::BuiltinVite => self.provider.marker,
        }
    }
}

/// The `vp doc info` report: `status` names the resolution state.
#[derive(Debug)]
pub enum DocInfoReport {
    Resolved(DocInfoResolved),
    NoProvider,
    MultipleProviders { candidates: Vec<&'static str> },
}

/// The JSON schema version; changes with the shape
/// (rfcs/doc-command.md, Selection reporting).
const SCHEMA_VERSION: u32 = 1;

impl DocInfoReport {
    /// The report resolves to a usable provider.
    pub fn resolved(&self) -> bool {
        matches!(self, DocInfoReport::Resolved(_))
    }

    /// The RFC's JSON shape. Keys are inserted in the documented order.
    pub fn to_json(&self) -> serde_json::Value {
        let mut report = serde_json::Map::new();
        report.insert("schemaVersion".into(), SCHEMA_VERSION.into());
        match self {
            DocInfoReport::Resolved(info) => {
                let provider = info.provider;
                report.insert("status".into(), "ready".into());
                report.insert("provider".into(), provider.id.into());
                report.insert("displayName".into(), provider.display_name.into());
                let mut source = serde_json::Map::new();
                source.insert(
                    "kind".into(),
                    match info.source {
                        SelectionSource::Config => "config".into(),
                        SelectionSource::Marker => "dependency-marker".into(),
                    },
                );
                report.insert("source".into(), source.into());
                let mut marker = serde_json::Map::new();
                marker.insert("package".into(), provider.marker.into());
                marker.insert("version".into(), json_version(info.marker_version.as_deref()));
                report.insert("marker".into(), marker.into());
                let mut execution = serde_json::Map::new();
                execution.insert("kind".into(), provider.target.as_str().into());
                execution.insert("package".into(), info.execution_package().into());
                execution.insert("version".into(), json_version(info.execution_version.as_deref()));
                if let ProviderTarget::PackageBin { bin_name, .. } = provider.target {
                    execution.insert("bin".into(), bin_name.into());
                }
                report.insert("execution".into(), execution.into());
                let mut compatibility = serde_json::Map::new();
                compatibility.insert("subject".into(), provider.marker.into());
                if let Some(range) = provider.version_range {
                    compatibility.insert("supportedRange".into(), range.into());
                }
                compatibility.insert("supported".into(), info.supported.into());
                report.insert("compatibility".into(), compatibility.into());
                report.insert(
                    "commands".into(),
                    provider
                        .capabilities
                        .iter()
                        .map(|action| action.as_str().into())
                        .collect::<Vec<serde_json::Value>>()
                        .into(),
                );
            }
            DocInfoReport::NoProvider => {
                report.insert("status".into(), "no-provider".into());
                report.insert("candidates".into(), Vec::<serde_json::Value>::new().into());
            }
            DocInfoReport::MultipleProviders { candidates } => {
                report.insert("status".into(), "multiple-providers".into());
                report.insert(
                    "candidates".into(),
                    candidates
                        .iter()
                        .map(|id| (*id).into())
                        .collect::<Vec<serde_json::Value>>()
                        .into(),
                );
            }
        }
        report.into()
    }
}

fn json_version(version: Option<&str>) -> serde_json::Value {
    version.map_or(serde_json::Value::Null, Into::into)
}

fn build_resolved(
    provider: &'static ProviderDefinition,
    source: SelectionSource,
    root: &Path,
) -> DocInfoResolved {
    let marker_version = find_installed_package(provider.marker, root)
        .as_ref()
        .and_then(|package| package.version())
        .map(str::to_string);
    let supported = marker_version.as_deref().is_some_and(|version| {
        provider.version_range.is_none_or(|range| resolve::version_satisfies(version, range))
    });
    let execution_package = match provider.target {
        ProviderTarget::PackageBin { package_name, .. } => package_name,
        ProviderTarget::BuiltinVite => provider.marker,
    };
    let execution_version = if execution_package == provider.marker {
        marker_version.clone()
    } else {
        find_installed_package(execution_package, root)
            .as_ref()
            .and_then(|package| package.version())
            .map(str::to_string)
    };
    DocInfoResolved { provider, source, marker_version, execution_version, supported }
}

/// Build the info report from the effective root. Reads only manifests and
/// the statically extracted config; an invalid `doc.provider` value or a
/// malformed manifest is a user error, the same as in the actions.
pub fn info_report(cwd: &Path, context: Option<&DocConfigContext>) -> Result<DocInfoReport, Error> {
    match resolve::select_provider(context, cwd)? {
        resolve::ProviderSelection::Selected { provider, source } => {
            Ok(DocInfoReport::Resolved(build_resolved(provider, source, cwd)))
        }
        resolve::ProviderSelection::NoProvider => Ok(DocInfoReport::NoProvider),
        resolve::ProviderSelection::Multiple(detected) => Ok(DocInfoReport::MultipleProviders {
            candidates: detected.iter().map(|provider| provider.id).collect(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn split_provider_reports_marker_and_execution_separately() {
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
        let DocInfoReport::Resolved(info) = &report else {
            panic!("expected a resolved report");
        };
        assert_eq!(info.provider.id, "starlight");
        assert_eq!(info.marker_version.as_deref(), Some("0.41.7"));
        assert_eq!(info.execution_package(), "astro");
        assert_eq!(info.execution_version.as_deref(), Some("7.2.2"));
        assert!(info.supported);
        let json = report.to_json();
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["status"], "ready");
        assert_eq!(json["source"]["kind"], "dependency-marker");
        assert_eq!(json["marker"]["package"], "@astrojs/starlight");
        assert_eq!(json["marker"]["version"], "0.41.7");
        assert_eq!(json["execution"]["kind"], "package-bin");
        assert_eq!(json["execution"]["package"], "astro");
        assert_eq!(json["execution"]["bin"], "astro");
        assert_eq!(json["compatibility"]["subject"], "@astrojs/starlight");
        assert_eq!(json["compatibility"]["supported"], true);
        assert_eq!(json["commands"][0], "dev");
    }

    #[test]
    fn multiple_markers_report_their_status_and_candidates() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "vitepress": "^2.0.0-0", "vocs": "^2.0.0" } }"#,
        )
        .unwrap();
        let report = info_report(dir.path(), None).unwrap();
        let DocInfoReport::MultipleProviders { candidates } = &report else {
            panic!("expected the multiple-providers state");
        };
        assert_eq!(candidates, &["vitepress", "vocs"]);
        let json = report.to_json();
        assert_eq!(json["status"], "multiple-providers");
        assert_eq!(json["candidates"][1], "vocs");
        assert!(!report.resolved());
    }

    #[test]
    fn a_malformed_manifest_is_an_error_not_a_state() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{ not json").unwrap();
        let error = info_report(dir.path(), None).unwrap_err();
        let Error::UserMessage(message) = error else {
            panic!("expected a user message");
        };
        assert!(message.contains("cannot parse"), "{message}");
        assert!(message.contains("package.json"), "{message}");
    }
}
