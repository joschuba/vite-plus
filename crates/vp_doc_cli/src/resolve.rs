//! Provider selection and action translation.

use std::path::{Path, PathBuf};

use crate::{
    cli::{DocAction, DocRequest},
    config::DocConfigContext,
    detect::{detect_providers, find_installed_package, find_nearest_manifest},
    error::{Error, user_message},
    providers::{DOC_PROVIDERS, ProviderDefinition, ProviderTarget, init_providers},
};

/// How the provider was selected (rfcs/doc-command.md, Provider Selection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionSource {
    /// `doc.provider` in the selected `vite.config.*`.
    Config,
    /// A unique dependency marker in the nearest package manifest.
    Marker,
}

impl SelectionSource {
    /// The subject for the not-installed diagnostic.
    fn subject(self) -> &'static str {
        match self {
            SelectionSource::Config => "`doc.provider`",
            SelectionSource::Marker => "`vp doc`",
        }
    }
}

/// The translated execution for an action request.
#[derive(Debug)]
pub enum DocResolution {
    /// Run the resolved bin file with the managed Node.js runtime.
    PackageBin { bin_path: PathBuf, args: Vec<String> },
    /// Run Vite+'s built-in Vite command with these arguments.
    BuiltinVite { args: Vec<String> },
}

/// A resolved action: the selection that produced it and the translated
/// execution. The caller renders the selection (the marker line) from the
/// same result that executes, so the two cannot diverge.
#[derive(Debug)]
pub struct DocExecution {
    pub provider: &'static ProviderDefinition,
    pub source: SelectionSource,
    pub resolution: DocResolution,
}

fn marker_list() -> String {
    DOC_PROVIDERS
        .iter()
        .map(|provider| match provider.marker_hint {
            Some(hint) => format!("  {} ({hint})", provider.marker),
            None => format!("  {}", provider.marker),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn lookup_provider(id: &str) -> Result<&'static ProviderDefinition, Error> {
    DOC_PROVIDERS.iter().find(|provider| provider.id == id).ok_or_else(|| {
        let supported =
            DOC_PROVIDERS.iter().map(|provider| provider.id).collect::<Vec<_>>().join(", ");
        user_message(format!(
            "`doc.provider` selects unknown documentation provider `{id}`\n\nSupported providers: {supported}"
        ))
    })
}

/// The outcome of provider selection when no rule failed.
#[derive(Debug)]
pub enum ProviderSelection {
    Selected {
        provider: &'static ProviderDefinition,
        source: SelectionSource,
    },
    /// No `doc.provider` and no dependency marker. An interactive caller
    /// can offer initialization; `resolve` reports
    /// [`no_provider_message`] instead.
    NoProvider,
}

/// The non-interactive no-provider diagnostic (rfcs/doc-command.md).
pub fn no_provider_message() -> String {
    let other_init = init_providers()
        .filter(|provider| provider.id != "vitepress")
        .map(|provider| format!("`vp doc init {}`", provider.id))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "no documentation provider is configured\n\nRun `vp doc init vitepress` to set up VitePress (recommended), or\n{other_init}.\n\nOr add one of these project dependencies yourself:\n{}\n\nIn a workspace, set `defaultPackage.doc` or run `vp -C <dir> doc` from the\ndocumentation package.",
        marker_list()
    )
}

/// Apply the selection order: `doc.provider`, then a unique dependency
/// marker in the nearest manifest from `root`. A package selects exactly
/// one provider.
pub fn select_provider(
    context: Option<&DocConfigContext>,
    root: &Path,
) -> Result<ProviderSelection, Error> {
    if let Some(id) = context.and_then(|context| context.config.provider.as_deref()) {
        let provider = lookup_provider(id)?;
        return Ok(ProviderSelection::Selected { provider, source: SelectionSource::Config });
    }

    let detected =
        find_nearest_manifest(root).map(|manifest| detect_providers(&manifest)).unwrap_or_default();
    match detected.as_slice() {
        [] => Ok(ProviderSelection::NoProvider),
        [provider] => Ok(ProviderSelection::Selected { provider, source: SelectionSource::Marker }),
        detected => {
            let ids = detected.iter().map(|provider| provider.id).collect::<Vec<_>>().join(", ");
            Err(user_message(format!(
                "multiple documentation providers are declared: {ids}\n\nRemove the markers you do not use, or set `doc.provider` in vite.config.ts\nduring a migration."
            )))
        }
    }
}

/// The capability gate: an unsupported action fails before process
/// creation. Every built-in provider declares all three actions; the field
/// exists so a provider without a native `dev` or `preview` can join
/// without a contract change (rfcs/doc-command.md, Built-in Providers).
fn ensure_capability(provider: &ProviderDefinition, action: DocAction) -> Result<(), Error> {
    if provider.capabilities.contains(&action) {
        return Ok(());
    }
    let supported =
        provider.capabilities.iter().map(|action| action.as_str()).collect::<Vec<_>>().join(", ");
    Err(user_message(format!(
        "the `{}` provider does not support `vp doc {}`\n\nSupported commands: {supported}",
        provider.id,
        action.as_str()
    )))
}

pub(crate) fn version_satisfies(version: &str, range: &str) -> bool {
    let Ok(version) = version.parse::<node_semver::Version>() else {
        return false;
    };
    let Ok(range) = range.parse::<node_semver::Range>() else {
        return false;
    };
    version.satisfies(&range)
}

/// Resolve an action request to a concrete execution. Fails with the
/// user-facing diagnostics from the RFC before any process is created.
pub fn resolve(
    request: &DocRequest,
    cwd: &Path,
    context: Option<&DocConfigContext>,
) -> Result<DocExecution, Error> {
    let (provider, source) = match select_provider(context, cwd)? {
        ProviderSelection::Selected { provider, source } => (provider, source),
        ProviderSelection::NoProvider => return Err(user_message(no_provider_message())),
    };

    ensure_capability(provider, request.action)?;

    let Some(marker) = find_installed_package(provider.marker, cwd) else {
        return Err(user_message(format!(
            "{} selects `{}`, but package `{}` is not installed",
            source.subject(),
            provider.id,
            provider.marker
        )));
    };

    if let Some(range) = provider.version_range {
        let version = marker.version();
        let satisfied = version.is_some_and(|version| version_satisfies(version, range));
        if !satisfied {
            return Err(user_message(format!(
                "`vp doc` supports {display}, but found {marker}@{version}\n\nInstall a {display} release (`{range}`).",
                display = provider.display_name,
                marker = provider.marker,
                version = version.unwrap_or("unknown"),
            )));
        }
    }

    let mut args = vec![request.action.as_str().to_string()];
    args.extend(request.args.iter().cloned());

    let resolution = match provider.target {
        ProviderTarget::BuiltinVite => DocResolution::BuiltinVite { args },
        ProviderTarget::PackageBin { package_name, bin_name } => {
            let executable = if package_name == provider.marker {
                marker
            } else {
                find_installed_package(package_name, cwd).ok_or_else(|| {
                    user_message(format!(
                        "provider `{}` executes `{package_name}`, but that package is not installed",
                        provider.id
                    ))
                })?
            };
            let bin = executable.package_json.get("bin");
            let bin_relative = match bin {
                Some(serde_json::Value::String(path)) => Some(path.as_str()),
                Some(serde_json::Value::Object(bins)) => {
                    bins.get(bin_name).and_then(|path| path.as_str())
                }
                _ => None,
            };
            let Some(bin_relative) = bin_relative else {
                return Err(user_message(format!(
                    "package `{package_name}` does not declare a `{bin_name}` bin"
                )));
            };
            DocResolution::PackageBin { bin_path: executable.root.join(bin_relative), args }
        }
    };
    Ok(DocExecution { provider, source, resolution })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;
    use crate::{cli::DocAction, config::DocConfig};

    fn write_manifest(dir: &Path, contents: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("package.json"), contents).unwrap();
    }

    fn install_package(dir: &Path, name: &str, manifest: &str) {
        let mut package_root = dir.join("node_modules");
        for segment in name.split('/') {
            package_root.push(segment);
        }
        fs::create_dir_all(&package_root).unwrap();
        fs::write(package_root.join("package.json"), manifest).unwrap();
    }

    fn install_vocs(dir: &Path) {
        install_package(
            dir,
            "vocs",
            r#"{ "name": "vocs", "version": "2.0.5", "bin": { "vocs": "bin.js" } }"#,
        );
    }

    fn context(dir: &Path, provider: Option<&str>) -> DocConfigContext {
        DocConfigContext {
            config: DocConfig { provider: provider.map(str::to_string) },
            config_dir: dir.to_path_buf(),
        }
    }

    fn build_request() -> DocRequest {
        DocRequest { action: DocAction::Build, args: Vec::new() }
    }

    #[test]
    fn config_provider_beats_markers() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            r#"{ "devDependencies": { "vitepress": "^2.0.0", "vocs": "^2.0.0" } }"#,
        );
        install_vocs(dir.path());
        let context = context(dir.path(), Some("vocs"));
        let resolution = resolve(&build_request(), dir.path(), Some(&context)).unwrap();
        let DocResolution::PackageBin { bin_path, args } = resolution.resolution else {
            panic!("expected a package-bin execution");
        };
        assert!(bin_path.ends_with("node_modules/vocs/bin.js"));
        assert_eq!(args, ["build"]);
    }

    #[test]
    fn peer_only_markers_are_not_detected() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), r#"{ "peerDependencies": { "vitepress": "^2.0.0-0" } }"#);
        let error = resolve(&build_request(), dir.path(), None).unwrap_err();
        let Error::UserMessage(message) = error else {
            panic!("expected a user message");
        };
        assert!(message.contains("no documentation provider is configured"), "{message}");
    }

    #[test]
    fn multiple_markers_are_a_misconfiguration() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            r#"{ "devDependencies": { "vitepress": "^2.0.0", "vocs": "^2.0.0" } }"#,
        );
        let error = resolve(&build_request(), dir.path(), None).unwrap_err();
        let Error::UserMessage(message) = error else {
            panic!("expected a user message");
        };
        assert!(message.contains("multiple documentation providers"), "{message}");
        assert!(message.contains("Remove the markers you do not use"), "{message}");
    }

    #[test]
    fn unknown_config_provider_is_a_user_error() {
        let dir = tempfile::tempdir().unwrap();
        let context = context(dir.path(), Some("typedoc"));
        let error = resolve(&build_request(), dir.path(), Some(&context)).unwrap_err();
        let Error::UserMessage(message) = error else {
            panic!("expected a user message");
        };
        assert!(message.contains("`doc.provider`"), "{message}");
        assert!(message.contains("typedoc"), "{message}");
    }

    #[test]
    fn config_provider_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), "{}");
        let context = context(dir.path(), Some("vocs"));
        let error = resolve(&build_request(), dir.path(), Some(&context)).unwrap_err();
        let Error::UserMessage(message) = error else {
            panic!("expected a user message");
        };
        // The RFC's Explicit provider validation example, verbatim.
        assert_eq!(message, "`doc.provider` selects `vocs`, but package `vocs` is not installed");
    }

    #[test]
    fn unsupported_action_fails_before_process_creation() {
        static LIMITED: ProviderDefinition = ProviderDefinition {
            id: "limited",
            display_name: "Limited",
            marker: "limited",
            marker_hint: None,
            version_range: None,
            vite_requirement: ">=8.0.0",
            capabilities: &[DocAction::Dev, DocAction::Build],
            target: ProviderTarget::PackageBin { package_name: "limited", bin_name: "limited" },
            init: None,
        };
        assert!(ensure_capability(&LIMITED, DocAction::Build).is_ok());
        let Error::UserMessage(message) = ensure_capability(&LIMITED, DocAction::Preview)
            .expect_err("preview is not a capability")
        else {
            panic!("expected a user message");
        };
        assert_eq!(
            message,
            "the `limited` provider does not support `vp doc preview`\n\nSupported commands: dev, build"
        );
    }

    #[test]
    fn every_builtin_provider_declares_all_actions() {
        for provider in DOC_PROVIDERS {
            for action in [DocAction::Dev, DocAction::Build, DocAction::Preview] {
                assert!(
                    provider.capabilities.contains(&action),
                    "{} lacks {action:?}",
                    provider.id
                );
            }
        }
    }

    #[test]
    fn vitepress_1_is_rejected_before_process_creation() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), r#"{ "devDependencies": { "vitepress": "^1.0.0" } }"#);
        install_package(
            dir.path(),
            "vitepress",
            r#"{ "name": "vitepress", "version": "1.6.4", "bin": { "vitepress": "bin/vitepress.js" } }"#,
        );
        let error = resolve(&build_request(), dir.path(), None).unwrap_err();
        let Error::UserMessage(message) = error else {
            panic!("expected a user message");
        };
        // The RFC's Unsupported tool version example, verbatim.
        assert_eq!(
            message,
            "`vp doc` supports VitePress, but found vitepress@1.6.4\n\nInstall a VitePress release (`>=2.0.0-alpha.18 <3.0.0`)."
        );
    }

    #[test]
    fn ox_content_resolves_the_builtin_vite_target() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            r#"{ "devDependencies": { "@ox-content/vite-plugin": "^1.1.0" } }"#,
        );
        install_package(
            dir.path(),
            "@ox-content/vite-plugin",
            r#"{ "name": "@ox-content/vite-plugin", "version": "1.1.0" }"#,
        );
        let request = DocRequest { action: DocAction::Dev, args: vec!["--open".to_string()] };
        let resolution = resolve(&request, dir.path(), None).unwrap();
        let DocResolution::BuiltinVite { args } = resolution.resolution else {
            panic!("expected the built-in Vite target");
        };
        assert_eq!(args, ["dev", "--open"]);
    }

    #[test]
    fn no_provider_message_renders_from_the_definitions() {
        let message = no_provider_message();
        assert!(message.contains("Run `vp doc init vitepress` to set up VitePress (recommended)"));
        assert!(message.contains("`vp doc init starlight`, `vp doc init ox-content`"));
        assert!(message.contains("  vitepress (major version 2)\n  vocs\n"));
        assert!(message.contains("  @astrojs/starlight"));
        assert!(message.contains("  @ox-content/vite-plugin"));
        assert!(message.contains("`defaultPackage.doc`"));
    }

    #[test]
    fn vitepress_floor_excludes_pre_vite8_alphas() {
        let range = ">=2.0.0-alpha.18 <3.0.0";
        assert!(!version_satisfies("2.0.0-alpha.17", range));
        assert!(version_satisfies("2.0.0-alpha.18", range));
        assert!(version_satisfies("2.0.0-alpha.19", range));
        assert!(version_satisfies("2.0.0", range));
        assert!(!version_satisfies("1.6.4", range));
    }

    #[test]
    fn every_provider_requirement_admits_the_bundled_vite_line() {
        for provider in DOC_PROVIDERS {
            assert!(
                version_satisfies("8.2.1", provider.vite_requirement),
                "{} declares `{}`",
                provider.id,
                provider.vite_requirement
            );
        }
    }

    #[test]
    fn starlight_resolves_the_astro_bin() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), r#"{ "devDependencies": { "@astrojs/starlight": "^0.41.0" } }"#);
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
        let resolution = resolve(&build_request(), dir.path(), None).unwrap();
        let DocResolution::PackageBin { bin_path, .. } = resolution.resolution else {
            panic!("expected a package-bin execution");
        };
        assert!(bin_path.ends_with("node_modules/astro/astro.js"));
    }

    #[test]
    fn starlight_missing_executable_package() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), r#"{ "devDependencies": { "@astrojs/starlight": "^0.41.0" } }"#);
        let marker_root = dir.path().join("node_modules/@astrojs/starlight");
        fs::create_dir_all(&marker_root).unwrap();
        fs::write(
            marker_root.join("package.json"),
            r#"{ "name": "@astrojs/starlight", "version": "0.41.7" }"#,
        )
        .unwrap();
        let error = resolve(&build_request(), dir.path(), None).unwrap_err();
        let Error::UserMessage(message) = error else {
            panic!("expected a user message");
        };
        assert_eq!(
            message,
            "provider `starlight` executes `astro`, but that package is not installed"
        );
    }

    #[test]
    fn detection_stays_at_the_invocation_directory() {
        // The workspace-root redirect happens before this crate runs (the
        // `defaultPackage` `doc` entry, an implicit `-C`), so resolution
        // never leaves the invocation directory on its own.
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), "{}");
        let docs = dir.path().join("packages/docs");
        write_manifest(&docs, r#"{ "devDependencies": { "vocs": "^2.0.0" } }"#);
        install_vocs(&docs);
        let error = resolve(&build_request(), dir.path(), None).unwrap_err();
        let Error::UserMessage(message) = error else {
            panic!("expected a user message");
        };
        assert!(message.contains("no documentation provider is configured"), "{message}");
        assert!(message.contains("`defaultPackage.doc`"), "{message}");

        let resolution = resolve(&build_request(), &docs, None).unwrap();
        let DocResolution::PackageBin { bin_path, .. } = resolution.resolution else {
            panic!("expected a package-bin execution");
        };
        assert!(bin_path.starts_with(&docs));
    }
}
