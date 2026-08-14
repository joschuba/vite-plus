//! Refusal for framework projects whose own CLI wraps Vite.
//!
//! `vp dev` and `vp build` run the bundled Vite CLI. Nuxt and Astro run Vite
//! only through their own CLIs. The bundled Vite CLI cannot serve or build
//! these projects: dev answers every URL with 404, and build stops on the
//! missing `index.html` entry. When a framework config file is next to the
//! nearest `package.json`, the two commands stop with an error and a hint
//! (voidzero-dev/vite-plus#1506). The hint points at the `package.json`
//! script that runs the framework command, or at the framework CLI through
//! `vp exec` when no script matches. An explicit `--config`/`-c` flag
//! selects a Vite config on purpose, so it skips the refusal.

use owo_colors::OwoColorize;
use vp_shared::output;
use vt::ExitStatus;
use vt_path::AbsolutePath;

use super::types::SynthesizableSubcommand;

/// Frameworks that wrap Vite behind their own CLI. Each entry lists the
/// config files that the framework's loader resolves, in its resolution
/// order.
const FRAMEWORKS: &[Framework] = &[
    // Nuxt resolves `nuxt.config` through c12. `loadNuxtConfig` passes
    // `configFile: "nuxt.config"`
    // (https://github.com/nuxt/nuxt/blob/v4.5.2/packages/kit/src/loader/config.ts),
    // and c12 tries the script extensions in `SUPPORTED_EXTENSIONS`
    // (https://github.com/unjs/c12/blob/v3.3.4/src/loader.ts). c12 also
    // accepts data configs (`.json`, `.jsonc`, `.json5`, `.yaml`, `.yml`,
    // `.toml`) and rc files. Those are rare, so the guard does not check
    // them.
    Framework {
        name: "Nuxt",
        config_files: &[
            "nuxt.config.js",
            "nuxt.config.ts",
            "nuxt.config.mjs",
            "nuxt.config.cjs",
            "nuxt.config.mts",
            "nuxt.config.cts",
        ],
        // The nuxt package ships the `nuxt` and `nuxi` bins (its `bin`
        // field, verified against nuxt 4.5.2).
        bins: &["nuxt", "nuxi"],
    },
    // Astro searches only these four names: `configPaths` in
    // https://github.com/withastro/astro/blob/astro@7.2.2/packages/astro/src/core/config/config.ts.
    // Astro does not load a `.cjs` or `.cts` config.
    Framework {
        name: "Astro",
        config_files: &[
            "astro.config.mjs",
            "astro.config.js",
            "astro.config.ts",
            "astro.config.mts",
        ],
        // The astro package ships the `astro` bin (its `bin` field,
        // verified against astro 7.2.2).
        bins: &["astro"],
    },
];

struct Framework {
    name: &'static str,
    config_files: &'static [&'static str],
    /// Executable names of the framework CLI. The first one is the name the
    /// `vp exec` hint shows.
    bins: &'static [&'static str],
}

/// Refuse `vp dev` / `vp build` in a Nuxt or Astro project.
///
/// Returns the exit status after it prints the refusal. Returns `None` when
/// the command can proceed.
pub(super) fn check(
    subcommand: &SynthesizableSubcommand,
    cwd: &AbsolutePath,
) -> Option<ExitStatus> {
    let (command, args) = match subcommand {
        SynthesizableSubcommand::Dev { args } => ("dev", args),
        SynthesizableSubcommand::Build { args } => ("build", args),
        _ => return None,
    };
    if has_explicit_config(args) {
        return None;
    }
    // `vp run` resolves the task from the nearest `package.json`. The same
    // walk here keeps the hint correct from a subdirectory.
    let package = vt_workspace::find_package_root(cwd).ok()?;
    let (framework, config_file) = detect(package.path)?;

    let built_in = format!("`vp {command}`").bright_blue().to_string();
    output::error(&format!(
        "this project uses {name} ({config_file}). {built_in} runs the bundled Vite CLI, \
         not the {name} CLI.",
        name = framework.name,
    ));
    let manifest = serde_json::from_slice::<serde_json::Value>(package.package_json.content())
        .unwrap_or(serde_json::Value::Null);
    output::raw_stderr(&format!("hint: {}", run_hint(&manifest, framework, command)));
    Some(ExitStatus(1))
}

/// The hint that follows the refusal. It points at the first path that works
/// in this package:
///
/// 1. the `package.json` script with the command's name,
/// 2. a script that runs the framework command under another name,
/// 3. the framework CLI through `vp exec`.
///
/// The check reads `package.json` scripts only. A `run.tasks` entry in
/// `vite.config.ts` with the command's name also works with `vp run`, but
/// the guard does not load that config.
fn run_hint(manifest: &serde_json::Value, framework: &Framework, command: &str) -> String {
    if let Some(scripts) = manifest.get("scripts").and_then(serde_json::Value::as_object) {
        if scripts.get(command).is_some_and(serde_json::Value::is_string) {
            let via_run = format!("`vp run {command}`").bright_blue().to_string();
            return format!("did you mean {via_run}?");
        }
        for (name, value) in scripts {
            let Some(value) = value.as_str() else { continue };
            for &bin in framework.bins {
                let framework_command = format!("{bin} {command}");
                if contains_word(value, &framework_command) {
                    let via_run = format!("`vp run {name}`").bright_blue().to_string();
                    return format!(
                        "did you mean {via_run}? The {name} script runs `{framework_command}`."
                    );
                }
            }
        }
    }
    let via_exec = format!("`vp exec {} {command}`", framework.bins[0]).bright_blue().to_string();
    format!("run the {} CLI with {via_exec}.", framework.name)
}

/// Whether `text` contains `pattern` between whitespace boundaries, so
/// `nuxt dev` does not match inside `nuxt devtools`.
fn contains_word(text: &str, pattern: &str) -> bool {
    let mut search_from = 0;
    while let Some(found) = text[search_from..].find(pattern) {
        let start = search_from + found;
        let end = start + pattern.len();
        let boundary_before = start == 0 || text[..start].ends_with(char::is_whitespace);
        let boundary_after = end == text.len() || text[end..].starts_with(char::is_whitespace);
        if boundary_before && boundary_after {
            return true;
        }
        search_from = start + 1;
    }
    false
}

/// The first framework config file present in `dir`.
fn detect(dir: &AbsolutePath) -> Option<(&'static Framework, &'static str)> {
    for framework in FRAMEWORKS {
        for &config_file in framework.config_files {
            if dir.join(config_file).as_path().is_file() {
                return Some((framework, config_file));
            }
        }
    }
    None
}

/// Whether the forwarded Vite args select a config file explicitly. The
/// capital `-C` flag retargets the directory. It is a different flag and
/// does not count.
fn has_explicit_config(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg == "-c" || arg == "--config" || arg.starts_with("--config=") || arg.starts_with("-c=")
    })
}

#[cfg(test)]
mod tests {
    use vt_path::AbsolutePathBuf;

    use super::{FRAMEWORKS, contains_word, detect, has_explicit_config, run_hint};

    fn framework(name: &str) -> &'static super::Framework {
        FRAMEWORKS.iter().find(|framework| framework.name == name).expect("known framework")
    }

    fn temp_dir(label: &str) -> AbsolutePathBuf {
        let unique = format!(
            "vp-framework-guard-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        AbsolutePathBuf::new(dir).expect("temp dir is absolute")
    }

    #[test]
    fn detects_nuxt_before_astro_across_extensions() {
        let dir = temp_dir("detect");
        std::fs::write(dir.as_path().join("astro.config.mjs"), "export default {}")
            .expect("write astro config");
        std::fs::write(dir.as_path().join("nuxt.config.mts"), "export default {}")
            .expect("write nuxt config");

        let (framework, config_file) = detect(&dir).expect("framework detected");
        assert_eq!(framework.name, "Nuxt");
        assert_eq!(config_file, "nuxt.config.mts");

        std::fs::remove_dir_all(dir.as_path()).expect("remove temp dir");
    }

    #[test]
    fn ignores_directories_and_unrelated_files() {
        let dir = temp_dir("ignore");
        std::fs::create_dir_all(dir.as_path().join("nuxt.config.ts")).expect("create dir");
        std::fs::write(dir.as_path().join("vite.config.ts"), "export default {}")
            .expect("write vite config");

        assert!(detect(&dir).is_none());

        std::fs::remove_dir_all(dir.as_path()).expect("remove temp dir");
    }

    #[test]
    fn ignores_config_names_astro_does_not_load() {
        let dir = temp_dir("astro-cjs");
        std::fs::write(dir.as_path().join("astro.config.cjs"), "module.exports = {}")
            .expect("write cjs config");
        std::fs::write(dir.as_path().join("astro.config.cts"), "module.exports = {}")
            .expect("write cts config");

        assert!(detect(&dir).is_none());

        std::fs::remove_dir_all(dir.as_path()).expect("remove temp dir");
    }

    #[test]
    fn hint_prefers_the_script_with_the_command_name() {
        let manifest = serde_json::json!({ "scripts": { "dev": "nuxt dev" } });
        let hint = run_hint(&manifest, framework("Nuxt"), "dev");
        assert!(hint.contains("vp run dev"), "hint was: {hint}");
    }

    #[test]
    fn hint_finds_a_renamed_script_that_runs_the_framework_command() {
        let manifest = serde_json::json!({ "scripts": {
            "devtools": "nuxt devtools enable",
            "start": "NODE_OPTIONS=--inspect nuxi dev --host",
        } });
        let hint = run_hint(&manifest, framework("Nuxt"), "dev");
        assert!(hint.contains("vp run start"), "hint was: {hint}");
        assert!(hint.contains("nuxi dev"), "hint was: {hint}");
    }

    #[test]
    fn hint_falls_back_to_vp_exec_without_a_matching_script() {
        let empty = serde_json::json!({});
        let hint = run_hint(&empty, framework("Nuxt"), "dev");
        assert!(hint.contains("vp exec nuxt dev"), "hint was: {hint}");

        let unrelated = serde_json::json!({ "scripts": { "lint": "oxlint ." } });
        let hint = run_hint(&unrelated, framework("Astro"), "build");
        assert!(hint.contains("vp exec astro build"), "hint was: {hint}");
    }

    #[test]
    fn contains_word_needs_whitespace_boundaries() {
        assert!(contains_word("nuxt dev", "nuxt dev"));
        assert!(contains_word("NODE_OPTIONS=--inspect nuxt dev --host", "nuxt dev"));
        assert!(!contains_word("nuxt devtools enable", "nuxt dev"));
        assert!(!contains_word("pnpm-nuxt dev", "nuxt dev"));
    }

    #[test]
    fn explicit_config_flags_skip_the_refusal() {
        let owned = |args: &[&str]| args.iter().map(|arg| (*arg).to_string()).collect::<Vec<_>>();
        assert!(has_explicit_config(&owned(&["--config", "vite.config.ts"])));
        assert!(has_explicit_config(&owned(&["--config=vite.config.ts"])));
        assert!(has_explicit_config(&owned(&["-c", "vite.config.ts"])));
        assert!(!has_explicit_config(&owned(&["--port", "5000"])));
        assert!(!has_explicit_config(&owned(&["-C", "apps/web"])));
    }
}
