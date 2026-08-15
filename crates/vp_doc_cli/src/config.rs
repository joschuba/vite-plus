//! The `doc` block of the selected `vite.config.*` (rfcs/doc-command.md,
//! Configuration).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, user_message};

/// The `doc` configuration block. The workspace-root package pointer is not
/// a `doc` key; it is the `doc` entry of `defaultPackage`, applied by the
/// caller as an implicit `-C` before this crate runs.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocConfig {
    /// Selects the provider ahead of dependency detection.
    pub provider: Option<String>,
}

/// A `doc` configuration together with the directory that contains the
/// selected config file.
#[derive(Debug, Clone)]
pub struct DocConfigContext {
    pub config: DocConfig,
    pub config_dir: PathBuf,
}

/// The static extraction of the `doc` field.
#[derive(Debug)]
pub enum StaticDocConfig {
    /// No `vite.config.*` exists from the start directory upward.
    Missing,
    /// The config file was analyzed; `doc` is this value (default when the
    /// field is absent).
    Resolved(DocConfigContext),
    /// The `doc` field is not statically analyzable. The caller must load
    /// the config through the JavaScript resolver and build the context with
    /// [`parse_doc_config`].
    NonStatic { config_dir: PathBuf },
}

/// Extract the `doc` field from the nearest `vite.config.*`, walking up from
/// the start directory. No JavaScript runs.
pub fn load_static_doc_config(start: &Path) -> Result<StaticDocConfig, Error> {
    let Some(config_dir) = find_config_dir(start) else {
        return Ok(StaticDocConfig::Missing);
    };
    let Some(abs) = vt_path::AbsolutePath::new(&config_dir) else {
        return Ok(StaticDocConfig::Missing);
    };
    match vp_static_config::resolve_static_config(abs).get("doc") {
        None => Ok(StaticDocConfig::Resolved(DocConfigContext {
            config: DocConfig::default(),
            config_dir,
        })),
        Some(vp_static_config::FieldValue::Json(value)) => {
            let config = parse_doc_config(value)?;
            Ok(StaticDocConfig::Resolved(DocConfigContext { config, config_dir }))
        }
        Some(vp_static_config::FieldValue::NonStatic) => {
            Ok(StaticDocConfig::NonStatic { config_dir })
        }
    }
}

/// Deserialize a JSON `doc` value with a user-facing shape error.
pub fn parse_doc_config(value: serde_json::Value) -> Result<DocConfig, Error> {
    serde_json::from_value(value)
        .map_err(|error| user_message(format!("invalid `doc` configuration: {error}")))
}

fn find_config_dir(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if let Some(abs) = vt_path::AbsolutePath::new(&dir)
            && vp_static_config::has_config_file(abs)
        {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn missing_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_static_doc_config(dir.path()).unwrap();
        assert!(matches!(loaded, StaticDocConfig::Missing));
    }

    #[test]
    fn static_doc_fields() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("vite.config.ts"),
            "export default { doc: { provider: 'vocs' } };\n",
        )
        .unwrap();
        let StaticDocConfig::Resolved(context) = load_static_doc_config(dir.path()).unwrap() else {
            panic!("expected a resolved static config");
        };
        assert_eq!(context.config.provider.as_deref(), Some("vocs"));
        assert_eq!(context.config_dir, dir.path());
    }

    #[test]
    fn absent_doc_field_resolves_to_default() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("vite.config.ts"), "export default {};\n").unwrap();
        let StaticDocConfig::Resolved(context) = load_static_doc_config(dir.path()).unwrap() else {
            panic!("expected a resolved static config");
        };
        assert!(context.config.provider.is_none());
    }

    #[test]
    fn non_static_doc_field() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("vite.config.ts"), "export default { doc: makeDoc() };\n")
            .unwrap();
        let loaded = load_static_doc_config(dir.path()).unwrap();
        assert!(matches!(loaded, StaticDocConfig::NonStatic { .. }));
    }

    #[test]
    fn walks_up_to_the_nearest_config() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("vite.config.ts"),
            "export default { doc: { provider: 'vocs' } };\n",
        )
        .unwrap();
        let nested = dir.path().join("packages/app");
        fs::create_dir_all(&nested).unwrap();
        let StaticDocConfig::Resolved(context) = load_static_doc_config(&nested).unwrap() else {
            panic!("expected a resolved static config");
        };
        assert_eq!(context.config_dir, dir.path());
    }
}
