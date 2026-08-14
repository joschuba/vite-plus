//! Provider selection and lifecycle translation.

use std::path::{Path, PathBuf};

use crate::{
    cli::DocRequest,
    detect::{detect_providers, find_installed_package, find_nearest_manifest},
    error::{Error, user_message},
    providers::{DOC_PROVIDERS, ProviderDefinition, ProviderTarget, init_providers},
};

/// The translated execution for a lifecycle request.
#[derive(Debug)]
pub enum DocResolution {
    /// Run the resolved bin file with the managed Node.js runtime.
    PackageBin { bin_path: PathBuf, args: Vec<String> },
    /// Run Vite+'s built-in Vite command with these arguments.
    BuiltinVite { args: Vec<String> },
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

fn select_provider(request: &DocRequest, cwd: &Path) -> Result<&'static ProviderDefinition, Error> {
    if let Some(id) = &request.provider {
        return DOC_PROVIDERS.iter().find(|provider| provider.id == id.as_str()).ok_or_else(|| {
            let supported =
                DOC_PROVIDERS.iter().map(|provider| provider.id).collect::<Vec<_>>().join(", ");
            user_message(format!(
                "unknown documentation provider `{id}`\n\nSupported providers: {supported}"
            ))
        });
    }

    let detected = find_nearest_manifest(cwd)
        .map(|nearest| detect_providers(&nearest.manifest))
        .unwrap_or_default();
    match detected.as_slice() {
        [] => {
            let other_init = init_providers()
                .filter(|provider| provider.id != "vitepress")
                .map(|provider| format!("`vp doc init {}`", provider.id))
                .collect::<Vec<_>>()
                .join(", ");
            Err(user_message(format!(
                "no documentation provider is configured\n\nRun `vp doc init vitepress` to set up VitePress (recommended), or\n{other_init}.\n\nOr add one of these project dependencies yourself:\n{}\n\nIn a workspace, run `vp -C <dir> doc` from the documentation package.",
                marker_list()
            )))
        }
        [provider] => Ok(provider),
        detected => {
            let ids = detected.iter().map(|provider| provider.id).collect::<Vec<_>>().join(", ");
            Err(user_message(format!(
                "multiple documentation providers are declared: {ids}\n\nPass `--provider` or run the command from the documentation package."
            )))
        }
    }
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

/// Resolve a lifecycle request to a concrete execution. Fails with the
/// user-facing diagnostics from the RFC before any process is created.
pub fn resolve(request: &DocRequest, cwd: &Path) -> Result<DocResolution, Error> {
    let provider = select_provider(request, cwd)?;

    if !provider.capabilities.contains(&request.action) {
        let supported = provider
            .capabilities
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(user_message(format!(
            "the `{}` provider does not support `vp doc {}`\n\nSupported commands: {supported}",
            provider.id,
            request.action.as_str()
        )));
    }

    let Some(marker) = find_installed_package(provider.marker, cwd) else {
        return Err(user_message(format!(
            "`vp doc` selects `{}`, but package `{}` is not installed",
            provider.id, provider.marker
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

    match provider.target {
        ProviderTarget::BuiltinVite => Ok(DocResolution::BuiltinVite { args }),
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
            Ok(DocResolution::PackageBin { bin_path: executable.root.join(bin_relative), args })
        }
    }
}
