//! `vp doc init` scaffolding. The caller prints the outcome and dispatches
//! the dependency install through the package-manager pipeline.

use std::{fs, path::Path};

use crate::{
    detect::{detect_providers, find_nearest_manifest},
    error::{Error, user_message},
    providers::{ProviderDefinition, init_providers},
};

/// A starter file the scaffold visited: created when missing, kept when it
/// already existed.
#[derive(Debug)]
pub struct ScaffoldedFile {
    pub path: &'static str,
    pub created: bool,
}

/// The scaffold result the caller renders and acts on.
#[derive(Debug)]
pub enum DocInitOutcome {
    AlreadyConfigured {
        provider: &'static ProviderDefinition,
    },
    Scaffolded {
        provider: &'static ProviderDefinition,
        /// Markers of other providers the project already declares.
        other_declared: Vec<&'static str>,
        files: Vec<ScaffoldedFile>,
        dependencies: &'static [&'static str],
    },
}

fn init_usage() -> String {
    let ids = init_providers().map(|provider| provider.id).collect::<Vec<_>>().join("|");
    format!("Usage: vp doc init <{ids}>")
}

/// Run the scaffold steps of `vp doc init`. Never overwrites existing files
/// and never installs packages; the returned outcome carries the dependency
/// specs for the caller's package-manager dispatch.
pub fn init_scaffold(args: &[String], cwd: &Path) -> Result<DocInitOutcome, Error> {
    let provider_id = args.first();
    let Some(provider_id) = provider_id.filter(|id| !id.starts_with('-')) else {
        return Err(user_message(format!(
            "`vp doc init` requires a provider ID\n\n{}",
            init_usage()
        )));
    };

    let Some(provider) =
        init_providers().find(|provider| provider.id == provider_id.as_str())
    else {
        return Err(user_message(format!(
            "unknown documentation provider `{provider_id}`\n\n{}",
            init_usage()
        )));
    };
    let init = provider.init.as_ref().expect("init_providers yields init-capable providers");

    let declared = find_nearest_manifest(cwd)
        .map(|nearest| detect_providers(&nearest.manifest))
        .unwrap_or_default();
    if declared.iter().any(|candidate| candidate.id == provider.id) {
        return Ok(DocInitOutcome::AlreadyConfigured { provider });
    }
    let other_declared = declared.iter().map(|candidate| candidate.marker).collect();

    let mut files = Vec::new();
    for file in init.starter_files {
        let target = cwd.join(file.path);
        if target.exists() {
            files.push(ScaffoldedFile { path: file.path, created: false });
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, file.content)?;
        files.push(ScaffoldedFile { path: file.path, created: true });
    }

    Ok(DocInitOutcome::Scaffolded {
        provider,
        other_declared,
        files,
        dependencies: init.dependencies,
    })
}
