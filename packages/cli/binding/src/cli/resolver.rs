use std::{env, ffi::OsStr, iter, sync::Arc};

use rustc_hash::FxHashMap;
use vt::config::user::{
    AutoTracking, EnabledCacheConfig, GlobWithBase, InputBase, UserCacheConfig, UserInputEntry,
};
use vt_path::AbsolutePath;
use vt_str::Str;

use super::{
    help::should_prepend_vitest_run,
    types::{CliOptions, ResolvedSubcommand, ResolvedUniversalViteConfig, SynthesizableSubcommand},
};

/// Resolves synthesizable subcommands to concrete programs and arguments.
/// Used by both direct CLI execution and CommandHandler.
pub struct SubcommandResolver {
    cli_options: Option<CliOptions>,
    workspace_path: Arc<AbsolutePath>,
}

impl std::fmt::Debug for SubcommandResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubcommandResolver")
            .field("has_cli_options", &self.cli_options.is_some())
            .field("workspace_path", &self.workspace_path)
            .finish()
    }
}

impl SubcommandResolver {
    pub fn new(workspace_path: Arc<AbsolutePath>) -> Self {
        Self { cli_options: None, workspace_path }
    }

    pub fn with_cli_options(mut self, cli_options: CliOptions) -> Self {
        self.cli_options = Some(cli_options);
        self
    }

    fn cli_options(&self) -> anyhow::Result<&CliOptions> {
        self.cli_options
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("CLI options not available (running without NAPI?)"))
    }

    pub(crate) async fn resolve_universal_vite_config(
        &self,
    ) -> anyhow::Result<ResolvedUniversalViteConfig> {
        let cli_options = self.cli_options()?;
        let workspace_path_str = self
            .workspace_path
            .as_path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("workspace path is not valid UTF-8"))?;
        let vite_config_json =
            (cli_options.resolve_universal_vite_config)(workspace_path_str.to_string()).await?;

        Ok(serde_json::from_str(&vite_config_json).inspect_err(|_| {
            tracing::error!("Failed to parse vite config: {vite_config_json}");
        })?)
    }

    /// Resolve a synthesizable subcommand to a concrete program, args, cache
    /// config, and envs. `cwd` is the execution directory the caller settled
    /// on (after `-C` and elicitation); the doc arm anchors detection there.
    pub(super) async fn resolve(
        &self,
        subcommand: SynthesizableSubcommand,
        resolved_vite_config: Option<&ResolvedUniversalViteConfig>,
        envs: &Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>>,
        cwd: &AbsolutePath,
    ) -> anyhow::Result<ResolvedSubcommand> {
        self.resolve_inner(subcommand, resolved_vite_config, envs, cwd).await
    }

    async fn resolve_inner(
        &self,
        subcommand: SynthesizableSubcommand,
        resolved_vite_config: Option<&ResolvedUniversalViteConfig>,
        envs: &Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>>,
        cwd: &AbsolutePath,
    ) -> anyhow::Result<ResolvedSubcommand> {
        match subcommand {
            SynthesizableSubcommand::Lint { mut args } => {
                let cli_options = self.cli_options()?;
                let resolved = (cli_options.lint)().await?;
                let js_path = resolved.bin_path;
                let js_path_str = js_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("lint JS path is not valid UTF-8"))?;
                let owned_resolved_vite_config;
                let resolved_vite_config = if let Some(config) = resolved_vite_config {
                    config
                } else {
                    owned_resolved_vite_config = self.resolve_universal_vite_config().await?;
                    &owned_resolved_vite_config
                };

                if let (Some(_), Some(config_file)) =
                    (&resolved_vite_config.lint, &resolved_vite_config.config_file)
                {
                    args.insert(0, "-c".to_string());
                    args.insert(1, config_file.clone());
                }

                Ok(ResolvedSubcommand {
                    program: Arc::from(OsStr::new("node")),
                    args: iter::once(Str::from("--disable-warning=MODULE_TYPELESS_PACKAGE_JSON"))
                        .chain(iter::once(Str::from(js_path_str)))
                        .chain(args.into_iter().map(Str::from))
                        .collect(),
                    cache_config: UserCacheConfig::with_config(EnabledCacheConfig {
                        env: Some(Box::new([Str::from("OXLINT_TSGOLINT_PATH")])),
                        untracked_env: None,
                        input: None,
                        output: None,
                    }),
                    envs: merge_resolved_envs_with_version(envs, resolved.envs),
                })
            }
            SynthesizableSubcommand::Fmt { mut args } => {
                let cli_options = self.cli_options()?;
                let resolved = (cli_options.fmt)().await?;
                let js_path = resolved.bin_path;
                let js_path_str = js_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("fmt JS path is not valid UTF-8"))?;
                let owned_resolved_vite_config;
                let resolved_vite_config = if let Some(config) = resolved_vite_config {
                    config
                } else {
                    owned_resolved_vite_config = self.resolve_universal_vite_config().await?;
                    &owned_resolved_vite_config
                };

                if let (Some(_), Some(config_file)) =
                    (&resolved_vite_config.fmt, &resolved_vite_config.config_file)
                {
                    args.insert(0, "-c".to_string());
                    args.insert(1, config_file.clone());
                }

                Ok(ResolvedSubcommand {
                    program: Arc::from(OsStr::new("node")),
                    args: iter::once(Str::from(js_path_str))
                        .chain(args.into_iter().map(Str::from))
                        .collect(),
                    cache_config: UserCacheConfig::with_config(EnabledCacheConfig {
                        env: None,
                        untracked_env: None,
                        input: None,
                        output: None,
                    }),
                    envs: merge_resolved_envs_with_version(envs, resolved.envs),
                })
            }
            SynthesizableSubcommand::Build { args } => {
                let cli_options = self.cli_options()?;
                let resolved = (cli_options.vite)().await?;
                let js_path = resolved.bin_path;
                let js_path_str = js_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("vite JS path is not valid UTF-8"))?;

                Ok(ResolvedSubcommand {
                    program: Arc::from(OsStr::new("node")),
                    args: iter::once(Str::from(js_path_str))
                        .chain(iter::once(Str::from("build")))
                        .chain(args.into_iter().map(Str::from))
                        .collect(),
                    // No synthetic cache config: vite reports its inputs/outputs/
                    // envs to the runner via `@voidzero-dev/vite-task-client`.
                    // All fields `None` keep caching enabled with auto input and
                    // auto output inference (the latter drives output restoration);
                    // vite's `ignoreInput`/`ignoreOutput`/`getEnv`/`getEnvs` refine
                    // the fingerprint at runtime.
                    cache_config: UserCacheConfig::with_config(EnabledCacheConfig {
                        env: None,
                        untracked_env: None,
                        input: None,
                        output: None,
                    }),
                    envs: merge_resolved_envs_with_version(envs, resolved.envs),
                })
            }
            SynthesizableSubcommand::Test { args } => {
                let cli_options = self.cli_options()?;
                let resolved = (cli_options.test)().await?;
                let js_path = resolved.bin_path;
                let js_path_str = js_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("test JS path is not valid UTF-8"))?;
                let prepend_run = should_prepend_vitest_run(&args);
                let vitest_args: Vec<Str> = if prepend_run {
                    iter::once(Str::from("run")).chain(args.into_iter().map(Str::from)).collect()
                } else {
                    args.into_iter().map(Str::from).collect()
                };

                Ok(ResolvedSubcommand {
                    program: Arc::from(OsStr::new("node")),
                    args: iter::once(Str::from(js_path_str)).chain(vitest_args).collect(),
                    cache_config: UserCacheConfig::with_config(EnabledCacheConfig {
                        env: None,
                        untracked_env: None,
                        input: Some(vec![
                            UserInputEntry::Auto(AutoTracking { auto: true }),
                            exclude_glob(
                                "!node_modules/.vite/vitest/**/results.json",
                                InputBase::Package,
                            ),
                        ]),
                        output: None,
                    }),
                    envs: merge_resolved_envs_with_version(envs, resolved.envs),
                })
            }
            SynthesizableSubcommand::Pack { args } => {
                let cli_options = self.cli_options()?;
                let resolved = (cli_options.pack)().await?;
                let js_path = resolved.bin_path;
                let js_path_str = js_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("pack JS path is not valid UTF-8"))?;

                Ok(ResolvedSubcommand {
                    program: Arc::from(OsStr::new("node")),
                    args: iter::once(Str::from(js_path_str))
                        .chain(args.into_iter().map(Str::from))
                        .collect(),
                    cache_config: UserCacheConfig::with_config(EnabledCacheConfig {
                        env: None,
                        untracked_env: None,
                        input: Some(build_pack_cache_inputs()),
                        output: None,
                    }),
                    envs: merge_resolved_envs(envs, resolved.envs),
                })
            }
            SynthesizableSubcommand::Dev { args } => {
                let cli_options = self.cli_options()?;
                let resolved = (cli_options.vite)().await?;
                let js_path = resolved.bin_path;
                let js_path_str = js_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("vite JS path is not valid UTF-8"))?;

                Ok(ResolvedSubcommand {
                    program: Arc::from(OsStr::new("node")),
                    args: iter::once(Str::from(js_path_str))
                        .chain(iter::once(Str::from("dev")))
                        .chain(args.into_iter().map(Str::from))
                        .collect(),
                    cache_config: UserCacheConfig::disabled(),
                    envs: merge_resolved_envs_with_version(envs, resolved.envs),
                })
            }
            SynthesizableSubcommand::Preview { args } => {
                let cli_options = self.cli_options()?;
                let resolved = (cli_options.vite)().await?;
                let js_path = resolved.bin_path;
                let js_path_str = js_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("vite JS path is not valid UTF-8"))?;

                Ok(ResolvedSubcommand {
                    program: Arc::from(OsStr::new("node")),
                    args: iter::once(Str::from(js_path_str))
                        .chain(iter::once(Str::from("preview")))
                        .chain(args.into_iter().map(Str::from))
                        .collect(),
                    cache_config: UserCacheConfig::disabled(),
                    envs: merge_resolved_envs_with_version(envs, resolved.envs),
                })
            }
            SynthesizableSubcommand::Doc { args } => {
                let request =
                    match vp_doc_cli::parse_doc_args(&args).map_err(|e| anyhow::anyhow!("{e}"))? {
                        vp_doc_cli::DocInvocation::Action(request) => request,
                        // The direct path handles `init`/`info` before resolution;
                        // reaching these arms means a task script.
                        vp_doc_cli::DocInvocation::Init { .. } => {
                            anyhow::bail!("`vp doc init` runs directly, not inside `vp run`")
                        }
                        vp_doc_cli::DocInvocation::Info { .. } => {
                            anyhow::bail!("`vp doc info` runs directly, not inside `vp run`")
                        }
                    };
                // Only `doc build` is a cacheable batch operation; the servers
                // stay uncached like `dev`/`preview`.
                let cache_config = if request.action.is_server() {
                    UserCacheConfig::disabled()
                } else {
                    UserCacheConfig::with_config(EnabledCacheConfig {
                        env: None,
                        untracked_env: None,
                        input: None,
                        output: None,
                    })
                };
                let context = load_doc_context(cwd.as_path(), self.cli_options.as_ref()).await?;
                let execution = vp_doc_cli::resolve(&request, cwd.as_path(), context.as_ref())
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                // When dependency detection made the selection, name the
                // marker before delegation; selections from `doc.provider`
                // stay silent (rfcs/doc-command.md, Selection reporting).
                // Stderr keeps the tool's stdout untouched.
                if execution.source == vp_doc_cli::SelectionSource::Marker {
                    eprintln!(
                        "Using provider `{}` (dependency marker `{}` in package.json)",
                        execution.provider.id, execution.provider.marker
                    );
                }

                match execution.resolution {
                    vp_doc_cli::DocResolution::PackageBin { bin_path, args } => {
                        let bin_path_str = bin_path
                            .to_str()
                            .ok_or_else(|| anyhow::anyhow!("doc tool path is not valid UTF-8"))?;
                        Ok(ResolvedSubcommand {
                            program: Arc::from(OsStr::new("node")),
                            args: iter::once(Str::from(bin_path_str))
                                .chain(args.iter().map(|arg| Str::from(arg.as_str())))
                                .collect(),
                            cache_config,
                            envs: merge_resolved_envs(
                                envs,
                                vec![("NODE_PACKAGE_MANAGER".to_string(), "vite-plus".to_string())],
                            ),
                        })
                    }
                    vp_doc_cli::DocResolution::BuiltinVite { args } => {
                        // The Vite-plugin provider reuses the same resolver as
                        // the top-level dev/build/preview commands.
                        let cli_options = self.cli_options()?;
                        let resolved = (cli_options.vite)().await?;
                        let js_path = resolved.bin_path;
                        let js_path_str = js_path
                            .to_str()
                            .ok_or_else(|| anyhow::anyhow!("vite JS path is not valid UTF-8"))?;
                        Ok(ResolvedSubcommand {
                            program: Arc::from(OsStr::new("node")),
                            args: iter::once(Str::from(js_path_str))
                                .chain(args.iter().map(|arg| Str::from(arg.as_str())))
                                .collect(),
                            cache_config,
                            envs: merge_resolved_envs_with_version(envs, resolved.envs),
                        })
                    }
                }
            }
            SynthesizableSubcommand::Check { .. } => {
                anyhow::bail!(
                    "Check is a composite command and cannot be resolved to a single subcommand"
                );
            }
        }
    }
}

/// Load the `doc` configuration context for the invocation directory: static
/// extraction first, then the JavaScript config resolver for non-static
/// configs. Without a JavaScript resolver, a non-static `doc` field falls
/// back to dependency detection.
pub(super) async fn load_doc_context(
    cwd: &std::path::Path,
    cli_options: Option<&CliOptions>,
) -> anyhow::Result<Option<vp_doc_cli::DocConfigContext>> {
    match vp_doc_cli::load_static_doc_config(cwd).map_err(anyhow::Error::new)? {
        vp_doc_cli::StaticDocConfig::Missing => Ok(None),
        vp_doc_cli::StaticDocConfig::Resolved(context) => Ok(Some(context)),
        vp_doc_cli::StaticDocConfig::NonStatic { config_dir } => {
            let Some(cli_options) = cli_options else {
                return Ok(None);
            };
            let dir = config_dir
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("config directory is not valid UTF-8"))?
                .to_string();
            let config_json = (cli_options.resolve_universal_vite_config)(dir).await?;
            let resolved: ResolvedUniversalViteConfig = serde_json::from_str(&config_json)
                .inspect_err(|_| {
                    tracing::error!("Failed to parse vite config: {config_json}");
                })?;
            let config = match resolved.doc {
                Some(value) => vp_doc_cli::parse_doc_config(value).map_err(anyhow::Error::new)?,
                None => vp_doc_cli::DocConfig::default(),
            };
            Ok(Some(vp_doc_cli::DocConfigContext { config, config_dir }))
        }
    }
}

/// Create a negative glob entry to exclude a pattern from cache fingerprinting.
fn exclude_glob(pattern: &str, base: InputBase) -> UserInputEntry {
    UserInputEntry::GlobWithBase(GlobWithBase { pattern: Str::from(pattern), base })
}

/// Common cache input entries for the pack command.
/// Excludes dist output files that are both read and written.
/// TODO: The hardcoded `!dist/**` exclusion is a temporary workaround. It will be replaced
/// by a runner-aware approach that automatically excludes task output directories.
fn build_pack_cache_inputs() -> Vec<UserInputEntry> {
    vec![
        UserInputEntry::Auto(AutoTracking { auto: true }),
        exclude_glob("!dist/**", InputBase::Package),
    ]
}

/// Cache input entries for the check command.
/// The vp check subprocess is a full vp CLI process (not resolved to a binary like
/// build/lint/fmt), so it accesses additional directories that must be excluded:
/// - `.vite/task-cache`: task runner state files that change after each run
pub(super) fn check_cache_inputs() -> Vec<UserInputEntry> {
    vec![
        UserInputEntry::Auto(AutoTracking { auto: true }),
        exclude_glob("!node_modules/.vite/task-cache/**", InputBase::Workspace),
        exclude_glob("!node_modules/.vite/task-cache/**", InputBase::Package),
    ]
}

fn merge_resolved_envs(
    envs: &Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>>,
    resolved_envs: Vec<(String, String)>,
) -> Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>> {
    let mut envs = FxHashMap::clone(envs);
    for (k, v) in resolved_envs {
        envs.entry(Arc::from(OsStr::new(&k))).or_insert_with(|| Arc::from(OsStr::new(&v)));
    }
    Arc::new(envs)
}

/// Merge resolved envs and inject VP_VERSION for rolldown-vite branding.
fn merge_resolved_envs_with_version(
    envs: &Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>>,
    resolved_envs: Vec<(String, String)>,
) -> Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>> {
    let mut merged = merge_resolved_envs(envs, resolved_envs);
    let map = Arc::make_mut(&mut merged);
    map.entry(Arc::from(OsStr::new("VP_VERSION")))
        .or_insert_with(|| Arc::from(OsStr::new(env!("CARGO_PKG_VERSION"))));
    merged
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use vt_path::AbsolutePathBuf;

    use super::*;

    /// A package that declares and installs a fake VitePress 2; the caller
    /// removes the directory.
    fn doc_fixture(name: &str) -> (std::path::PathBuf, AbsolutePathBuf) {
        let suffix =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time should be valid").as_nanos();
        let dir = std::env::temp_dir().join(format!("vite-plus-doc-cache-{name}-{suffix}"));
        let bin_dir = dir.join("node_modules/vitepress/bin");
        fs::create_dir_all(&bin_dir).expect("fixture should be created");
        fs::write(
            dir.join("package.json"),
            r#"{ "name": "fixture", "devDependencies": { "vitepress": "^2.0.0-0" } }"#,
        )
        .expect("package.json should be written");
        fs::write(
            dir.join("node_modules/vitepress/package.json"),
            r#"{ "name": "vitepress", "version": "2.0.0-alpha.19", "bin": { "vitepress": "bin/vitepress.js" } }"#,
        )
        .expect("installed manifest should be written");
        fs::write(bin_dir.join("vitepress.js"), "").expect("bin should be written");
        let abs = AbsolutePathBuf::new(dir.clone()).expect("fixture dir should be absolute");
        (dir, abs)
    }

    async fn resolve_doc(args: &[&str], cwd: &AbsolutePathBuf) -> ResolvedSubcommand {
        let workspace: Arc<AbsolutePath> = cwd.clone().into();
        let resolver = SubcommandResolver::new(workspace);
        let envs: Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>> = Arc::new(FxHashMap::default());
        let subcommand = SynthesizableSubcommand::Doc {
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
        };
        resolver.resolve(subcommand, None, &envs, cwd).await.expect("the doc action should resolve")
    }

    #[tokio::test]
    async fn doc_build_is_cached_and_the_servers_are_not() {
        let (dir, cwd) = doc_fixture("policy");
        let build = resolve_doc(&["build"], &cwd).await;
        assert!(matches!(build.cache_config, UserCacheConfig::Enabled { .. }));
        // The resolved execution is the package bin through the managed
        // runtime.
        assert_eq!(build.program.as_ref(), OsStr::new("node"));
        assert!(build.args[0].as_str().ends_with("vitepress.js"), "{:?}", build.args);
        assert_eq!(build.args[1].as_str(), "build");

        let dev = resolve_doc(&["dev"], &cwd).await;
        assert!(matches!(dev.cache_config, UserCacheConfig::Disabled { .. }));
        let preview = resolve_doc(&["preview"], &cwd).await;
        assert!(matches!(preview.cache_config, UserCacheConfig::Disabled { .. }));
        fs::remove_dir_all(dir).expect("fixture should be removed");
    }
}
