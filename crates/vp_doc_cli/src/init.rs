//! `vp doc init` scaffolding. The caller prints the outcome and dispatches
//! the dependency install through the package-manager pipeline.

use std::{fs, path::Path};

use crate::{
    detect::{detect_providers, find_installed_package, find_nearest_manifest},
    error::{Error, user_message},
    providers::{ProviderDefinition, ProviderTarget, init_providers},
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
        files: Vec<ScaffoldedFile>,
        dependencies: &'static [&'static str],
    },
}

/// The outcome of init step 3: write `doc` configuration only when
/// detection alone would not select the provider afterward
/// (rfcs/doc-command.md, Initialization).
#[derive(Debug, PartialEq, Eq)]
pub enum DocConfigWrite {
    /// Detection selects the provider by itself; no config is needed.
    NotNeeded,
    /// A new `vite.config.ts` was created with the `doc.provider` entry.
    Created,
    /// The existing config file gained the `doc.provider` entry.
    Updated { file: String },
    /// The existing config could not be edited safely; the caller prints
    /// the manual instruction.
    Manual { file: String },
}

fn init_usage() -> String {
    let ids = init_providers().map(|provider| provider.id).collect::<Vec<_>>().join("|");
    format!("Usage: vp doc init <{ids}>")
}

/// Run the scaffold steps of `vp doc init`. Never overwrites existing files
/// and never installs packages; the returned outcome carries the dependency
/// specs for the caller's package-manager dispatch. A non-interactive
/// session must name the provider; the interactive caller supplies the
/// picker's choice.
pub fn init_scaffold(provider_id: Option<&str>, cwd: &Path) -> Result<DocInitOutcome, Error> {
    let Some(provider_id) = provider_id else {
        return Err(user_message(format!(
            "`vp doc init` requires a provider ID\n\n{}",
            init_usage()
        )));
    };

    let Some(provider) = init_providers().find(|provider| provider.id == provider_id) else {
        return Err(user_message(format!(
            "unknown documentation provider `{provider_id}`\n\n{}",
            init_usage()
        )));
    };
    let init = provider.init.as_ref().expect("init_providers yields init-capable providers");

    let declared =
        find_nearest_manifest(cwd)?.map(|manifest| detect_providers(&manifest)).unwrap_or_default();
    if declared.iter().any(|candidate| candidate.id == provider.id)
        && provider_runnable(provider, cwd)
    {
        return Ok(DocInitOutcome::AlreadyConfigured { provider });
    }
    // A declared but not runnable provider is the residue of a failed
    // install; fall through so the retry repairs it instead of reporting
    // success (rfcs/doc-command.md, Initialization).

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

    Ok(DocInitOutcome::Scaffolded { provider, files, dependencies: init.dependencies })
}

/// Runnable means the marker package resolves, and a package-bin
/// provider's executable package resolves too. Starter files are not
/// required: a pre-existing project keeps its own content and config.
fn provider_runnable(provider: &ProviderDefinition, cwd: &Path) -> bool {
    if find_installed_package(provider.marker, cwd).is_none() {
        return false;
    }
    match provider.target {
        ProviderTarget::PackageBin { package_name, .. } => {
            package_name == provider.marker || find_installed_package(package_name, cwd).is_some()
        }
        ProviderTarget::BuiltinVite => true,
    }
}

/// Init step 3: write `doc` configuration to the effective root's Vite
/// config only when detection alone would not select the provider, for
/// example when another marker is already declared. Runs after the
/// dependency install, so the nearest manifest reflects the final state.
///
/// The target file comes from `vp_static_config::resolve_config_path`, the
/// same priority order resolution reads, so the written config is the one
/// later invocations load.
pub fn write_doc_provider_config(
    provider: &ProviderDefinition,
    cwd: &Path,
) -> Result<DocConfigWrite, Error> {
    let detected =
        find_nearest_manifest(cwd)?.map(|manifest| detect_providers(&manifest)).unwrap_or_default();
    if matches!(detected.as_slice(), [only] if only.id == provider.id) {
        return Ok(DocConfigWrite::NotNeeded);
    }

    let entry = format!("doc: {{\n    provider: '{}',\n  }},", provider.id);
    let existing = vt_path::AbsolutePath::new(cwd).and_then(vp_static_config::resolve_config_path);
    let Some(path) = existing else {
        let content = format!(
            "import {{ defineConfig }} from 'vite-plus';\n\nexport default defineConfig({{\n  {entry}\n}});\n"
        );
        fs::write(cwd.join("vite.config.ts"), content)?;
        return Ok(DocConfigWrite::Created);
    };

    let file = path
        .as_path()
        .file_name()
        .map_or_else(|| "vite.config.ts".to_string(), |name| name.to_string_lossy().into_owned());
    let contents = fs::read_to_string(path.as_path())?;
    // A config that already carries a `doc` key needs a human decision;
    // never overwrite a stated provider. The check is AST-based, so a `doc`
    // substring in a comment or a task name does not block the insert. An
    // unparsable config falls to the manual instruction.
    match vp_migration::has_config_key(&contents, "doc") {
        Ok(false) => {}
        Ok(true) | Err(_) => return Ok(DocConfigWrite::Manual { file }),
    }
    for opening in ["export default defineConfig({", "export default {"] {
        if let Some(index) = contents.find(opening) {
            let insert_at = index + opening.len();
            let mut updated = String::with_capacity(contents.len() + entry.len() + 4);
            updated.push_str(&contents[..insert_at]);
            updated.push_str("\n  ");
            updated.push_str(&entry);
            updated.push_str(&contents[insert_at..]);
            fs::write(path.as_path(), updated)?;
            return Ok(DocConfigWrite::Updated { file });
        }
    }
    Ok(DocConfigWrite::Manual { file })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::Error, providers::DOC_PROVIDERS};

    fn provider(id: &str) -> &'static ProviderDefinition {
        DOC_PROVIDERS.iter().find(|provider| provider.id == id).unwrap()
    }

    #[test]
    fn init_requires_an_explicit_provider_id() {
        let dir = tempfile::tempdir().unwrap();
        let Error::UserMessage(message) = init_scaffold(None, dir.path()).unwrap_err() else {
            panic!("expected a user message");
        };
        assert!(message.contains("requires a provider ID"), "{message}");
        // The usage line renders from the init-capable definitions.
        assert!(message.contains("vp doc init <vitepress|starlight|ox-content>"), "{message}");
    }

    #[test]
    fn init_rejects_an_unknown_provider() {
        let dir = tempfile::tempdir().unwrap();
        let Error::UserMessage(message) = init_scaffold(Some("typedoc"), dir.path()).unwrap_err()
        else {
            panic!("expected a user message");
        };
        assert!(message.contains("unknown documentation provider `typedoc`"), "{message}");
    }

    #[test]
    fn scaffold_creates_missing_files_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("astro.config.mjs"), "// existing config\n").unwrap();
        let DocInitOutcome::Scaffolded { provider, files, dependencies } =
            init_scaffold(Some("starlight"), dir.path()).unwrap()
        else {
            panic!("expected a scaffold");
        };
        assert_eq!(provider.id, "starlight");
        assert_eq!(dependencies, &["astro", "@astrojs/starlight"]);
        let existing = files.iter().find(|file| file.path == "astro.config.mjs").unwrap();
        assert!(!existing.created);
        assert_eq!(
            fs::read_to_string(dir.path().join("astro.config.mjs")).unwrap(),
            "// existing config\n"
        );
        let page = files.iter().find(|file| file.path == "src/content/docs/index.md").unwrap();
        assert!(page.created);
        assert!(dir.path().join("src/content/docs/index.md").is_file());
    }

    #[test]
    fn second_init_reports_the_existing_setup_only_when_runnable() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "vitepress": "^2.0.0-0" } }"#,
        )
        .unwrap();
        // Declared but not installed: the residue of a failed install.
        // The retry repairs it (scaffold plus reinstall) instead of
        // reporting success (rfcs/doc-command.md, Initialization).
        let outcome = init_scaffold(Some("vitepress"), dir.path()).unwrap();
        assert!(matches!(outcome, DocInitOutcome::Scaffolded { .. }));

        let package_root = dir.path().join("node_modules/vitepress");
        fs::create_dir_all(&package_root).unwrap();
        fs::write(
            package_root.join("package.json"),
            r#"{ "name": "vitepress", "version": "2.0.0-alpha.19", "bin": { "vitepress": "bin/vitepress.js" } }"#,
        )
        .unwrap();
        let outcome = init_scaffold(Some("vitepress"), dir.path()).unwrap();
        assert!(matches!(
            outcome,
            DocInitOutcome::AlreadyConfigured { provider } if provider.id == "vitepress"
        ));
    }

    #[test]
    fn config_write_is_not_needed_for_a_unique_marker() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "vitepress": "^2.0.0-0" } }"#,
        )
        .unwrap();
        let write = write_doc_provider_config(provider("vitepress"), dir.path()).unwrap();
        assert_eq!(write, DocConfigWrite::NotNeeded);
        assert!(!dir.path().join("vite.config.ts").exists());
    }

    #[test]
    fn config_write_creates_a_missing_config() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "vitepress": "^2.0.0-0", "vocs": "^2.0.0" } }"#,
        )
        .unwrap();
        let write = write_doc_provider_config(provider("vocs"), dir.path()).unwrap();
        assert_eq!(write, DocConfigWrite::Created);
        let contents = fs::read_to_string(dir.path().join("vite.config.ts")).unwrap();
        assert!(contents.contains("provider: 'vocs'"), "{contents}");
    }

    #[test]
    fn config_write_inserts_into_define_config() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "vitepress": "^2.0.0-0", "vocs": "^2.0.0" } }"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("vite.config.ts"),
            "import { defineConfig } from 'vite-plus';\n\nexport default defineConfig({\n  run: {},\n});\n",
        )
        .unwrap();
        let write = write_doc_provider_config(provider("vocs"), dir.path()).unwrap();
        assert_eq!(write, DocConfigWrite::Updated { file: "vite.config.ts".to_string() });
        let contents = fs::read_to_string(dir.path().join("vite.config.ts")).unwrap();
        assert!(
            contents.contains(
                "export default defineConfig({\n  doc: {\n    provider: 'vocs',\n  },\n  run: {},\n});"
            ),
            "{contents}"
        );
    }

    #[test]
    fn config_write_targets_the_file_resolution_reads() {
        // Vite resolves vite.config.js before vite.config.ts; the write must
        // land in the file later invocations load.
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "vitepress": "^2.0.0-0", "vocs": "^2.0.0" } }"#,
        )
        .unwrap();
        fs::write(dir.path().join("vite.config.js"), "export default {\n};\n").unwrap();
        fs::write(dir.path().join("vite.config.ts"), "export default {\n};\n").unwrap();
        let write = write_doc_provider_config(provider("vocs"), dir.path()).unwrap();
        assert_eq!(write, DocConfigWrite::Updated { file: "vite.config.js".to_string() });
        let js = fs::read_to_string(dir.path().join("vite.config.js")).unwrap();
        assert!(js.contains("provider: 'vocs'"), "{js}");
        let ts = fs::read_to_string(dir.path().join("vite.config.ts")).unwrap();
        assert!(!ts.contains("provider"), "{ts}");
    }

    #[test]
    fn config_write_ignores_a_doc_substring_outside_the_config_object() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "vitepress": "^2.0.0-0", "vocs": "^2.0.0" } }"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("vite.config.ts"),
            "// doc: not a config key\nexport default {\n  run: { tasks: { doc: { command: 'x' } } },\n};\n",
        )
        .unwrap();
        let write = write_doc_provider_config(provider("vocs"), dir.path()).unwrap();
        // `run.tasks.doc` is not a top-level `doc` key; the insert proceeds.
        assert_eq!(write, DocConfigWrite::Updated { file: "vite.config.ts".to_string() });
    }

    #[test]
    fn config_write_never_touches_an_existing_doc_block() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "vitepress": "^2.0.0-0", "vocs": "^2.0.0" } }"#,
        )
        .unwrap();
        let original = "export default {\n  doc: {\n    provider: 'vitepress',\n  },\n};\n";
        fs::write(dir.path().join("vite.config.ts"), original).unwrap();
        let write = write_doc_provider_config(provider("vocs"), dir.path()).unwrap();
        assert_eq!(write, DocConfigWrite::Manual { file: "vite.config.ts".to_string() });
        assert_eq!(fs::read_to_string(dir.path().join("vite.config.ts")).unwrap(), original);
    }

    #[test]
    fn config_write_falls_back_to_manual_for_an_unrecognized_shape() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "vitepress": "^2.0.0-0", "vocs": "^2.0.0" } }"#,
        )
        .unwrap();
        let original = "export default makeConfig();\n";
        fs::write(dir.path().join("vite.config.mts"), original).unwrap();
        let write = write_doc_provider_config(provider("vocs"), dir.path()).unwrap();
        assert_eq!(write, DocConfigWrite::Manual { file: "vite.config.mts".to_string() });
        assert_eq!(fs::read_to_string(dir.path().join("vite.config.mts")).unwrap(), original);
    }
}
