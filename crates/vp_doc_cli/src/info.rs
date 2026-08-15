//! `vp doc info`: report the resolved provider without starting the tool.

use std::path::Path;

use crate::{
    config::DocConfigContext,
    detect::find_installed_package,
    error::Error,
    providers::{ProviderDefinition, ProviderTarget},
    resolve::{self, SelectionSource},
};

/// The tool half of the report: the executable package. For a host-bin
/// provider such as Starlight this differs from the marker; the version
/// gate (`supported_range`/`version_supported`) stays on the marker.
#[derive(Debug)]
pub struct DocToolInfo {
    pub package: &'static str,
    pub version: Option<String>,
    pub supported_range: Option<&'static str>,
    pub version_supported: bool,
}

/// A report that resolved to a usable provider.
#[derive(Debug)]
pub struct DocInfoResolved {
    pub provider: &'static ProviderDefinition,
    pub source: SelectionSource,
    pub tool: DocToolInfo,
}

/// The `vp doc info` report: either a resolved provider or the unresolved
/// state with its candidates (empty for no provider, several for multiple
/// markers).
#[derive(Debug)]
pub enum DocInfoReport {
    Resolved(DocInfoResolved),
    Unresolved { candidates: Vec<&'static str> },
}

impl DocInfoReport {
    /// The report resolves to a usable provider.
    pub fn resolved(&self) -> bool {
        matches!(self, DocInfoReport::Resolved(_))
    }

    /// The RFC's JSON shape (rfcs/doc-command.md, Selection reporting).
    /// Keys are inserted in the documented order.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            DocInfoReport::Resolved(info) => {
                let mut source = serde_json::Map::new();
                match info.source {
                    SelectionSource::Config => {
                        source.insert("kind".into(), "config".into());
                    }
                    SelectionSource::Marker => {
                        source.insert("kind".into(), "dependency-marker".into());
                        source.insert("marker".into(), info.provider.marker.into());
                    }
                }
                let mut tool = serde_json::Map::new();
                tool.insert("package".into(), info.tool.package.into());
                tool.insert(
                    "version".into(),
                    info.tool.version.as_deref().map_or(serde_json::Value::Null, Into::into),
                );
                if let Some(range) = info.tool.supported_range {
                    tool.insert("supportedRange".into(), range.into());
                }
                tool.insert("versionSupported".into(), info.tool.version_supported.into());
                let mut report = serde_json::Map::new();
                report.insert("provider".into(), info.provider.id.into());
                report.insert("displayName".into(), info.provider.display_name.into());
                report.insert("source".into(), source.into());
                report.insert("target".into(), info.provider.target.as_str().into());
                report.insert("tool".into(), tool.into());
                report.insert(
                    "commands".into(),
                    info.provider
                        .capabilities
                        .iter()
                        .map(|action| action.as_str().into())
                        .collect::<Vec<serde_json::Value>>()
                        .into(),
                );
                report.into()
            }
            DocInfoReport::Unresolved { candidates } => {
                let mut report = serde_json::Map::new();
                report.insert("provider".into(), serde_json::Value::Null);
                report.insert(
                    "candidates".into(),
                    candidates
                        .iter()
                        .map(|id| (*id).into())
                        .collect::<Vec<serde_json::Value>>()
                        .into(),
                );
                report.into()
            }
        }
    }
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
    let version_supported = marker_version.as_deref().is_some_and(|version| {
        provider.version_range.is_none_or(|range| resolve::version_satisfies(version, range))
    });
    let tool_package = match provider.target {
        ProviderTarget::PackageBin { package_name, .. } => package_name,
        ProviderTarget::BuiltinVite => provider.marker,
    };
    let tool_version = if tool_package == provider.marker {
        marker_version
    } else {
        find_installed_package(tool_package, root)
            .as_ref()
            .and_then(|package| package.version())
            .map(str::to_string)
    };
    DocInfoResolved {
        provider,
        source,
        tool: DocToolInfo {
            package: tool_package,
            version: tool_version,
            supported_range: provider.version_range,
            version_supported,
        },
    }
}

/// Build the info report from the effective root. Reads only manifests and
/// the statically extracted config. An invalid `doc.provider` value is a
/// user error, the same as in the actions.
pub fn info_report(cwd: &Path, context: Option<&DocConfigContext>) -> Result<DocInfoReport, Error> {
    match resolve::select_provider(context, cwd) {
        Ok(resolve::ProviderSelection::Selected { provider, source }) => {
            Ok(DocInfoReport::Resolved(build_resolved(provider, source, cwd)))
        }
        Err(Error::UserMessage(message))
            if context.is_some_and(|context| context.config.provider.is_some()) =>
        {
            Err(Error::UserMessage(message))
        }
        Ok(resolve::ProviderSelection::NoProvider) | Err(_) => Ok(DocInfoReport::Unresolved {
            candidates: crate::detect::find_nearest_manifest(cwd)
                .map(|manifest| crate::detect::detect_providers(&manifest))
                .unwrap_or_default()
                .iter()
                .map(|provider| provider.id)
                .collect(),
        }),
    }
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
        let DocInfoReport::Resolved(info) = &report else {
            panic!("expected a resolved report");
        };
        assert_eq!(info.provider.id, "starlight");
        assert_eq!(info.tool.package, "astro");
        assert_eq!(info.tool.version.as_deref(), Some("7.2.2"));
        // The version gate stays on the marker package.
        assert!(info.tool.version_supported);
        let json = report.to_json();
        assert_eq!(json["provider"], "starlight");
        assert_eq!(json["source"]["kind"], "dependency-marker");
        assert_eq!(json["source"]["marker"], "@astrojs/starlight");
        assert_eq!(json["tool"]["package"], "astro");
        assert_eq!(json["commands"][0], "dev");
    }

    #[test]
    fn unresolved_report_lists_the_candidates() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "vitepress": "^2.0.0-0", "vocs": "^2.0.0" } }"#,
        )
        .unwrap();
        let report = info_report(dir.path(), None).unwrap();
        let DocInfoReport::Unresolved { candidates } = &report else {
            panic!("expected an unresolved report");
        };
        assert_eq!(candidates, &["vitepress", "vocs"]);
        let json = report.to_json();
        assert_eq!(json["provider"], serde_json::Value::Null);
        assert_eq!(json["candidates"][1], "vocs");
    }
}
