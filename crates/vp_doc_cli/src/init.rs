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
    Updated { file: &'static str },
    /// The existing config could not be edited safely; the caller prints
    /// the manual instruction.
    Manual { file: &'static str },
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

    let Some(provider) = init_providers().find(|provider| provider.id == provider_id.as_str())
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

const CONFIG_FILE_NAMES: &[&str] =
    &["vite.config.ts", "vite.config.mts", "vite.config.js", "vite.config.mjs"];

/// Init step 3: write `doc` configuration to the effective root's Vite
/// config only when detection alone would not select the provider, for
/// example when another marker is already declared. Runs after the
/// dependency install, so the nearest manifest reflects the final state.
pub fn write_doc_provider_config(
    provider: &ProviderDefinition,
    cwd: &Path,
) -> Result<DocConfigWrite, Error> {
    let detected = find_nearest_manifest(cwd)
        .map(|nearest| detect_providers(&nearest.manifest))
        .unwrap_or_default();
    if matches!(detected.as_slice(), [only] if only.id == provider.id) {
        return Ok(DocConfigWrite::NotNeeded);
    }

    let entry = format!("doc: {{\n    provider: '{}',\n  }},", provider.id);
    let Some(file) = CONFIG_FILE_NAMES.iter().copied().find(|name| cwd.join(name).exists()) else {
        let content = format!(
            "import {{ defineConfig }} from 'vite-plus';\n\nexport default defineConfig({{\n  {entry}\n}});\n"
        );
        fs::write(cwd.join("vite.config.ts"), content)?;
        return Ok(DocConfigWrite::Created);
    };

    let path = cwd.join(file);
    let contents = fs::read_to_string(&path)?;
    // A config that already carries a `doc` block needs a human decision;
    // never overwrite a stated provider.
    if contents.contains("doc:") {
        return Ok(DocConfigWrite::Manual { file });
    }
    for opening in ["export default defineConfig({", "export default {"] {
        if let Some(index) = contents.find(opening) {
            let insert_at = index + opening.len();
            let mut updated = String::with_capacity(contents.len() + entry.len() + 4);
            updated.push_str(&contents[..insert_at]);
            updated.push_str("\n  ");
            updated.push_str(&entry);
            updated.push_str(&contents[insert_at..]);
            fs::write(&path, updated)?;
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

    fn scaffold(args: &[&str], cwd: &Path) -> Result<DocInitOutcome, Error> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        init_scaffold(&args, cwd)
    }

    #[test]
    fn init_requires_an_explicit_provider_id() {
        let dir = tempfile::tempdir().unwrap();
        let Error::UserMessage(message) = scaffold(&[], dir.path()).unwrap_err() else {
            panic!("expected a user message");
        };
        assert!(message.contains("requires a provider ID"), "{message}");
        // The usage line renders from the init-capable definitions.
        assert!(message.contains("vp doc init <vitepress|starlight|ox-content>"), "{message}");
    }

    #[test]
    fn init_rejects_an_unknown_provider() {
        let dir = tempfile::tempdir().unwrap();
        let Error::UserMessage(message) = scaffold(&["typedoc"], dir.path()).unwrap_err() else {
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
            scaffold(&["starlight"], dir.path()).unwrap()
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
    fn second_init_reports_the_existing_setup() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "vitepress": "^2.0.0-0" } }"#,
        )
        .unwrap();
        let outcome = scaffold(&["vitepress"], dir.path()).unwrap();
        assert!(matches!(
            outcome,
            DocInitOutcome::AlreadyConfigured { provider } if provider.id == "vitepress"
        ));
        assert!(!dir.path().join("index.md").exists());
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
        assert_eq!(write, DocConfigWrite::Updated { file: "vite.config.ts" });
        let contents = fs::read_to_string(dir.path().join("vite.config.ts")).unwrap();
        assert!(
            contents.contains(
                "export default defineConfig({\n  doc: {\n    provider: 'vocs',\n  },\n  run: {},\n});"
            ),
            "{contents}"
        );
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
        assert_eq!(write, DocConfigWrite::Manual { file: "vite.config.ts" });
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
        assert_eq!(write, DocConfigWrite::Manual { file: "vite.config.mts" });
        assert_eq!(fs::read_to_string(dir.path().join("vite.config.mts")).unwrap(), original);
    }
}
