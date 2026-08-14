//! Refusal for framework projects whose own CLI wraps Vite.
//!
//! `vp dev` and `vp build` run the bundled Vite CLI. Nuxt and Astro use Vite
//! under the hood but only through their own CLIs, so the bundled Vite CLI
//! cannot serve or build them: dev answers every URL with 404 and build stops
//! on the missing `index.html` entry. A framework config file next to the
//! nearest `package.json` turns both commands into an error that points at
//! `vp run <command>` (voidzero-dev/vite-plus#1506). An explicit
//! `--config`/`-c` flag skips the refusal: it selects a Vite config on
//! purpose, so the bundled Vite CLI stays reachable.

use owo_colors::OwoColorize;
use vp_shared::output;
use vt::ExitStatus;
use vt_path::AbsolutePath;

use super::types::SynthesizableSubcommand;

/// Frameworks with a Vite-wrapping CLI, marked by the config file next to
/// `package.json` that their own CLI loads. Each list mirrors that loader's
/// own file names, in its resolution order.
const FRAMEWORKS: &[Framework] = &[
    // Nuxt resolves `nuxt.config` through c12: `loadNuxtConfig` passes
    // `configFile: "nuxt.config"`
    // (https://github.com/nuxt/nuxt/blob/v4.5.2/packages/kit/src/loader/config.ts)
    // and c12 tries the script extensions of `SUPPORTED_EXTENSIONS`
    // (https://github.com/unjs/c12/blob/v3.3.4/src/loader.ts). c12 also
    // accepts data configs (`.json`, `.jsonc`, `.json5`, `.yaml`, `.yml`,
    // `.toml`) and rc files; those are rare enough for the guard to leave
    // alone.
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
    },
    // Astro searches exactly these four names: `configPaths` in
    // https://github.com/withastro/astro/blob/astro@7.2.2/packages/astro/src/core/config/config.ts.
    // Astro loads no `.cjs`/`.cts` config.
    Framework {
        name: "Astro",
        config_files: &[
            "astro.config.mjs",
            "astro.config.js",
            "astro.config.ts",
            "astro.config.mts",
        ],
    },
];

struct Framework {
    name: &'static str,
    config_files: &'static [&'static str],
}

/// Refuse `vp dev` / `vp build` in a Nuxt or Astro project.
///
/// Returns the exit status to stop with once the refusal printed, or `None`
/// when the command should proceed.
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
    // The nearest `package.json` is the package `vp run` resolves the task
    // from, so the refusal and its hint stay consistent from a subdirectory.
    let package = vt_workspace::find_package_root(cwd).ok()?;
    let (framework, config_file) = detect(package.path)?;

    let built_in = format!("`vp {command}`").bright_blue().to_string();
    let via_run = format!("`vp run {command}`").bright_blue().to_string();
    output::error(&format!(
        "this project uses {name} ({config_file}), but {built_in} runs the bundled Vite CLI, \
         not the {name} CLI.",
        name = framework.name,
    ));
    output::raw_stderr(&format!("hint: did you mean {via_run}?"));
    Some(ExitStatus(1))
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
/// capital `-C` retarget flag is a different flag and does not count.
fn has_explicit_config(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg == "-c" || arg == "--config" || arg.starts_with("--config=") || arg.starts_with("-c=")
    })
}

#[cfg(test)]
mod tests {
    use vt_path::AbsolutePathBuf;

    use super::{detect, has_explicit_config};

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
    fn explicit_config_flags_skip_the_refusal() {
        let owned = |args: &[&str]| args.iter().map(|arg| (*arg).to_string()).collect::<Vec<_>>();
        assert!(has_explicit_config(&owned(&["--config", "vite.config.ts"])));
        assert!(has_explicit_config(&owned(&["--config=vite.config.ts"])));
        assert!(has_explicit_config(&owned(&["-c", "vite.config.ts"])));
        assert!(!has_explicit_config(&owned(&["--port", "5000"])));
        assert!(!has_explicit_config(&owned(&["-C", "apps/web"])));
    }
}
