//! CLI types and logic for vite-plus using the new Session API from vite-task.
//!
//! This module contains all the CLI-related code.
//! It handles argument parsing, command dispatching, and orchestration of the task execution.

mod app_target;
mod execution;
mod handler;
mod help;
mod resolver;
mod script_note;
mod types;

use std::{borrow::Cow, env, ffi::OsStr, sync::Arc};

use clap::Parser;
use cow_utils::CowUtils;
pub(crate) use execution::resolve_and_capture_output;
// Re-exports for lib.rs and check/mod.rs
pub use resolver::SubcommandResolver;
use rustc_hash::FxHashMap;
pub(crate) use types::CapturedCommandOutput;
pub use types::{
    BoxedResolverFn, CliOptions, ResolveCommandResult, SynthesizableSubcommand, ToolchainArgs,
    ViteConfigResolverFn,
};
use vp_error::Error;
pub use vp_shared::init_tracing;
use vp_shared::{PrependOptions, env_vars, prepend_to_path_env};
use vt::{ExitStatus, Session, SessionConfig};
use vt_path::{AbsolutePath, AbsolutePathBuf};
use vt_str::Str;

use self::{
    execution::{FilterStream, resolve_and_execute, resolve_and_execute_with_filter},
    handler::{VitePlusCommandHandler, VitePlusConfigLoader},
    help::{
        handle_cli_parse_error, normalize_help_args, print_help, should_print_help,
        should_suppress_subcommand_stdout,
    },
    types::CLIArgs,
};

/// Execute a synthesizable subcommand directly (not through vite-task Session).
/// No caching, no task graph, no dependency resolution. `pre_retargeted` is
/// true when the caller already applied an elicitation retarget (the doc
/// dispatch), so the PWD handling below matches an app-command retarget.
async fn execute_direct_subcommand(
    subcommand: SynthesizableSubcommand,
    cwd: &AbsolutePathBuf,
    options: Option<CliOptions>,
    pre_retargeted: bool,
) -> Result<ExitStatus, Error> {
    // A bare app command at a workspace root resolves its target first
    // (defaultPackage, package listing); the command then runs as if invoked
    // in the resolved directory (rfcs/cwd-flag.md).
    let (target, workspace_root_hint) = app_target::resolve_app_target(&subcommand, cwd)?;
    let retargeted = pre_retargeted || matches!(&target, app_target::AppTarget::Dir(_));
    let cwd = match &target {
        app_target::AppTarget::Exit(status) => return Ok(*status),
        app_target::AppTarget::Dir(dir) => dir,
        app_target::AppTarget::CurrentDir => cwd,
    };

    // The resolver hands back the workspace root it already found whenever the
    // command runs in the unchanged cwd (never after a -C/elicitation
    // retarget), so it matches a fresh lookup here and saves the second walk.
    let workspace_root = match workspace_root_hint {
        Some(root) => root,
        None => vt_workspace::find_workspace_root(cwd)?.0,
    };
    let workspace_path: Arc<AbsolutePath> = workspace_root.path.into();

    let resolver = if let Some(options) = options {
        SubcommandResolver::new(Arc::clone(&workspace_path)).with_cli_options(options)
    } else {
        SubcommandResolver::new(Arc::clone(&workspace_path))
    };

    let envs: Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>> = Arc::new({
        let mut envs: FxHashMap<Arc<OsStr>, Arc<OsStr>> = std::env::vars_os()
            .map(|(k, v)| (Arc::from(k.as_os_str()), Arc::from(v.as_os_str())))
            .collect();
        // When elicitation retargeted the command, the tool runs with the
        // target as its working directory: keep the POSIX PWD consistent,
        // like a real `cd`. Untargeted runs keep the caller's PWD verbatim
        // (it may legitimately differ from cwd through shell symlinks).
        if cfg!(unix) && retargeted {
            envs.insert(Arc::from(OsStr::new("PWD")), Arc::from(cwd.as_path().as_os_str()));
        }
        envs
    });
    let envs = envs_with_explicit_package_manager_path(cwd, envs).await?;

    let status = match subcommand {
        SynthesizableSubcommand::Check {
            fix,
            no_fmt,
            no_lint,
            no_error_on_unmatched_pattern,
            paths,
        } => {
            return crate::check::execute_check(
                &resolver,
                fix,
                no_fmt,
                no_lint,
                no_error_on_unmatched_pattern,
                paths,
                &envs,
                cwd,
            )
            .await;
        }
        other => {
            if should_suppress_subcommand_stdout(&other) {
                resolve_and_execute_with_filter(
                    &resolver,
                    other,
                    None,
                    &envs,
                    cwd,
                    FilterStream::Stdout,
                    |_| Cow::Borrowed(""),
                )
                .await?
            } else if matches!(&other, SynthesizableSubcommand::Fmt { .. }) {
                resolve_and_execute_with_filter(
                    &resolver,
                    other,
                    None,
                    &envs,
                    cwd,
                    FilterStream::Stderr,
                    |s| s.cow_replace("oxfmt --init", "vp fmt --init"),
                )
                .await?
            } else {
                resolve_and_execute(&resolver, other, None, &envs, cwd).await?
            }
        }
    };

    Ok(status)
}

fn is_path_env_key(key: &OsStr) -> bool {
    if cfg!(windows) { key.eq_ignore_ascii_case("PATH") } else { key == "PATH" }
}

fn try_prepend_to_env_path(
    envs: &Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>>,
    bin_prefix: &AbsolutePath,
) -> Result<Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>>, Error> {
    let path_key = envs
        .keys()
        .find(|key| is_path_env_key(key.as_ref()))
        .cloned()
        .unwrap_or_else(|| Arc::from(OsStr::new("PATH")));
    let current_path =
        envs.get(&path_key).map_or_else(Default::default, |path| path.to_os_string());
    let paths = if current_path.is_empty() {
        Vec::new()
    } else {
        env::split_paths(&current_path).collect::<Vec<_>>()
    };

    if paths.first().is_some_and(|path| path == bin_prefix.as_path()) {
        return Ok(Arc::clone(envs));
    }

    let new_path = env::join_paths(
        std::iter::once(bin_prefix.as_path().to_path_buf()).chain(paths.into_iter()),
    )
    .map_err(|error| Error::Anyhow(anyhow::Error::new(error)))?;

    let mut envs = FxHashMap::clone(envs);
    envs.insert(path_key, Arc::from(new_path.as_os_str()));
    Ok(Arc::new(envs))
}

fn prepend_to_env_path(
    envs: &Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>>,
    bin_prefix: &AbsolutePath,
) -> Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>> {
    match try_prepend_to_env_path(envs, bin_prefix) {
        Ok(updated_envs) => updated_envs,
        Err(error) => {
            tracing::debug!(
                ?error,
                "failed to prepend managed package manager bin to direct command PATH"
            );
            Arc::clone(envs)
        }
    }
}

async fn envs_with_explicit_package_manager_path(
    cwd: &AbsolutePath,
    envs: Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>>,
) -> Result<Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>>, Error> {
    let Some(resolution) = (match vp_pm_cli::resolve_package_manager_from_package_json(cwd) {
        Ok(resolution) => resolution,
        Err(error) => {
            tracing::debug!(
                ?error,
                "failed to resolve explicit packageManager for direct command PATH setup"
            );
            return Ok(envs);
        }
    }) else {
        return Ok(envs);
    };

    let (install_dir, _, _) = match vp_pm_cli::download_package_manager(
        resolution.package_manager_type,
        &resolution.version,
        resolution.hash.as_deref(),
    )
    .await
    {
        Ok(result) => result,
        // A missing package manager has other causes, such as no network or an
        // unknown version. The command still runs from PATH, so those causes
        // stay a debug log. An integrity failure is different. If vp hides it,
        // the user sees only "command not found" further down.
        Err(error) if error.is_integrity_failure() => return Err(error),
        Err(error) => {
            tracing::debug!(
                ?error,
                "failed to ensure managed package manager for direct command PATH setup"
            );
            return Ok(envs);
        }
    };

    Ok(prepend_to_env_path(&envs, &install_dir.join("bin")))
}

/// Execute a vite-task command (run, cache) through Session.
async fn execute_vite_task_command(
    command: vt::Command,
    cwd: AbsolutePathBuf,
    options: Option<CliOptions>,
) -> Result<ExitStatus, Error> {
    let (workspace_root, _) = vt_workspace::find_workspace_root(&cwd)?;
    let workspace_path: Arc<AbsolutePath> = workspace_root.path.into();

    let resolve_vite_config_fn = options
        .as_ref()
        .map(|o| Arc::clone(&o.resolve_universal_vite_config))
        .ok_or_else(|| {
            Error::Anyhow(anyhow::anyhow!(
                "resolve_universal_vite_config is required but not available"
            ))
        })?;

    let resolver = if let Some(options) = options {
        SubcommandResolver::new(Arc::clone(&workspace_path)).with_cli_options(options)
    } else {
        SubcommandResolver::new(Arc::clone(&workspace_path))
    };

    let mut command_handler = VitePlusCommandHandler::new(resolver);
    let mut config_loader = VitePlusConfigLoader::new(resolve_vite_config_fn);

    // Update PATH to include package manager bin directory BEFORE session init
    match vp_pm_cli::PackageManager::builder(&cwd).build().await {
        Ok(pm) => {
            let bin_prefix = pm.get_bin_prefix();
            let _ = prepend_to_path_env(&bin_prefix, PrependOptions::default());
        }
        Err(error) if error.is_integrity_failure() => return Err(error),
        Err(error) => {
            tracing::debug!(?error, "failed to resolve package manager for task PATH setup");
        }
    }

    let session = Session::init(SessionConfig {
        command_handler: &mut command_handler,
        user_config_loader: &mut config_loader,
        program_name: Str::from("vp"),
    })?;

    // Main execution (consumes session). vite-task prints any errors itself
    // and returns only an exit status.
    let status = session.main(command).await;

    Ok(status)
}

fn execute_toolchain_command(
    args: ToolchainArgs,
    options: Option<&CliOptions>,
) -> Result<ExitStatus, Error> {
    if args.global {
        vp_shared::output::error("The `--global` option requires the global `vp` CLI");
        return Ok(ExitStatus(1));
    }

    let options = options.ok_or_else(|| {
        Error::Anyhow(anyhow::anyhow!("this CLI does not include toolchain metadata"))
    })?;
    let manifest_path = toolchain_manifest_path(options).ok_or_else(|| {
        Error::Anyhow(anyhow::anyhow!("the toolchain manifest path must be absolute"))
    })?;
    let manifest = vp_toolchain::load_manifest(&manifest_path).map_err(anyhow::Error::new)?;
    let version = vp_toolchain::root_version(&manifest).ok_or_else(|| {
        Error::Anyhow(anyhow::anyhow!("toolchain manifest does not contain vite-plus"))
    })?;
    let source = vp_toolchain::Source {
        scope: vp_toolchain::Scope::Local,
        path: options.vite_plus_package_path.clone().into(),
        vite_plus_version: version.into(),
    };
    let report = match vp_toolchain::build_report(&manifest, &args.tools, source) {
        Ok(report) => report,
        Err(vp_toolchain::ToolchainError::UnknownFilter(filter)) => {
            let message = format!("`{filter}` is not in the Vite+ toolchain");
            if args.json {
                vp_shared::output::raw_stderr(&format!("error: {message}"));
            } else {
                vp_shared::output::error(&message);
                vp_shared::output::raw_stderr(&format!(
                    "hint: run `vp why {filter}` to show project dependencies"
                ));
            }
            return Ok(ExitStatus(1));
        }
        Err(error) => return Err(anyhow::Error::new(error).into()),
    };

    let rendered = if args.json {
        vp_toolchain::render_json(&report).map_err(anyhow::Error::new)?
    } else {
        vp_toolchain::render(&report)
    };
    vp_shared::output::raw_inline(&rendered);
    Ok(ExitStatus::SUCCESS)
}

fn toolchain_manifest_path(options: &CliOptions) -> Option<AbsolutePathBuf> {
    AbsolutePathBuf::new(options.toolchain_manifest_path.clone().into())
}

fn print_toolchain_why_hint(options: Option<&CliOptions>, packages: &[String]) {
    let Some(manifest) = options
        .and_then(toolchain_manifest_path)
        .and_then(|path| vp_toolchain::load_manifest(&path).ok())
    else {
        return;
    };
    let Some(hint) = vp_toolchain::why_hint(&manifest, packages) else {
        return;
    };
    vp_shared::output::raw_stderr("");
    vp_shared::output::raw_stderr(&hint);
}

/// Main entry point for vite-plus CLI.
///
/// # Arguments
/// * `cwd` - Current working directory
/// * `options` - Optional CLI options with resolver functions
/// * `args` - Optional CLI arguments. If None, uses env::args(). This allows NAPI bindings
///            to pass process.argv.slice(2) to avoid including node binary and script path.
#[tracing::instrument(skip(options))]
pub async fn main(
    cwd: AbsolutePathBuf,
    options: Option<CliOptions>,
    args: Option<Vec<String>>,
) -> Result<ExitStatus, Error> {
    let raw_args: Vec<String> = args.unwrap_or_else(|| env::args().skip(1).collect());
    // The global CLI resolves aliases to their canonical names before
    // delegating, so prefer the original spelling it forwards. A direct local
    // invocation can use its first, still-unnormalized argument.
    let raw_subcommand =
        env::var(env_vars::VP_RAW_SUBCOMMAND).ok().or_else(|| raw_args.first().cloned());
    let args_vec = normalize_help_args(raw_args);
    if should_print_help(&args_vec) {
        print_help();
        return Ok(ExitStatus::SUCCESS);
    }

    let args_with_program = std::iter::once("vp".to_string()).chain(args_vec.iter().cloned());
    let cli_args = match CLIArgs::try_parse_from(args_with_program) {
        Ok(args) => args,
        Err(err) => return handle_cli_parse_error(err),
    };

    match cli_args {
        CLIArgs::Synthesizable(subcmd) => {
            if let SynthesizableSubcommand::Doc { args } = &subcmd {
                match dispatch_doc(args, &cwd, options.as_ref()).await? {
                    Some(DocDispatch::Done(status)) => return Ok(status),
                    Some(DocDispatch::Run { cwd: doc_cwd, retargeted }) => {
                        script_note::print(raw_subcommand.as_deref(), &doc_cwd);
                        return execute_direct_subcommand(subcmd, &doc_cwd, options, retargeted)
                            .await;
                    }
                    // Parse errors fall through so the resolver reports them.
                    None => {}
                }
            }
            // Only the built-ins can be mistaken for a script. `run`/`cache`
            // below are the script path itself; `install` and friends
            // legitimately trigger a project's `install` lifecycle scripts
            // through the package manager, so redirecting those to `vpr` would
            // be wrong; and `exec` names a binary rather than a task.
            script_note::print(raw_subcommand.as_deref(), &cwd);
            execute_direct_subcommand(subcmd, &cwd, options, false).await
        }
        CLIArgs::ViteTask(command) => execute_vite_task_command(command, cwd, options).await,
        CLIArgs::PackageManager(pm) => execute_pm_command(pm, &cwd, options.as_ref()).await,
        CLIArgs::Exec(exec_args) => crate::exec::execute(exec_args, &cwd).await,
        CLIArgs::Toolchain(args) => execute_toolchain_command(args, options.as_ref()),
    }
}

/// How the `vp doc` dispatch concluded.
enum DocDispatch {
    /// The invocation finished (init, info, an elicitation exit, or a
    /// declined no-provider offer).
    Done(ExitStatus),
    /// Run the action through the generic direct path at this directory.
    /// `retargeted` is true when the redirect or the picker moved it.
    Run { cwd: AbsolutePathBuf, retargeted: bool },
}

/// The `vp doc` pre-pass (rfcs/doc-command.md): the `defaultPackage` `doc`
/// redirect and the documentation-package elicitation settle the effective
/// root, `init` and `info` execute here, and an interactive action without
/// a provider gets the initialization offer. Returns `None` on a parse
/// error so the resolver reports it.
async fn dispatch_doc(
    args: &[String],
    cwd: &AbsolutePathBuf,
    options: Option<&CliOptions>,
) -> Result<Option<DocDispatch>, Error> {
    let Ok(invocation) = vp_doc_cli::parse_doc_args(args) else {
        return Ok(None);
    };
    let command = match &invocation {
        vp_doc_cli::DocInvocation::Action(request) => match request.action {
            vp_doc_cli::DocAction::Dev => "doc",
            vp_doc_cli::DocAction::Build => "doc build",
            vp_doc_cli::DocAction::Preview => "doc preview",
        },
        vp_doc_cli::DocInvocation::Init { .. } => "doc init",
        vp_doc_cli::DocInvocation::Info { .. } => "doc info",
    };
    let elicit = matches!(invocation, vp_doc_cli::DocInvocation::Action(_));
    let target = app_target::resolve_doc_target(cwd, command, elicit)?;
    let retargeted = matches!(&target, app_target::AppTarget::Dir(_));
    let cwd = match target {
        app_target::AppTarget::Exit(status) => return Ok(Some(DocDispatch::Done(status))),
        app_target::AppTarget::Dir(dir) => dir,
        app_target::AppTarget::CurrentDir => cwd.clone(),
    };
    let status = match invocation {
        vp_doc_cli::DocInvocation::Init { provider } => {
            execute_doc_init(provider, &cwd, options, true).await?
        }
        vp_doc_cli::DocInvocation::Info { json } => execute_doc_info(json, &cwd, options).await?,
        vp_doc_cli::DocInvocation::Action(_) => {
            // Interactive no-provider flow: offer initialization, then
            // continue with the requested action (rfcs/doc-command.md, No
            // provider).
            match offer_doc_init_for_action(&cwd, options).await? {
                Some(exit) => exit,
                None => return Ok(Some(DocDispatch::Run { cwd, retargeted })),
            }
        }
    };
    Ok(Some(DocDispatch::Done(status)))
}

/// Render a `vp_doc_cli` error: user messages print behind the `error:`
/// prefix and exit 1; anything else propagates.
fn render_doc_error(error: vp_doc_cli::Error) -> Result<ExitStatus, Error> {
    match error {
        vp_doc_cli::Error::UserMessage(message) => {
            eprintln!("error: {message}");
            Ok(ExitStatus(1))
        }
        other => Err(Error::Anyhow(anyhow::Error::new(other))),
    }
}

/// Select prompt over the init-capable providers, VitePress recommended
/// first. Returns the chosen provider, or `None` on cancel. Reuses the
/// shared picker and its milestone protocol.
fn run_doc_provider_picker() -> Result<Option<&'static vp_doc_cli::ProviderDefinition>, Error> {
    let providers: Vec<&'static vp_doc_cli::ProviderDefinition> =
        vp_doc_cli::init_providers().collect();
    let items: Vec<vt_select::SelectItem> = providers
        .iter()
        .enumerate()
        .map(|(index, provider)| {
            let hint = provider.init.as_ref().map_or("", |init| init.prompt_hint);
            let description =
                if index == 0 { vt_str::format!("{hint} · recommended") } else { Str::from(hint) };
            vt_select::SelectItem {
                label: vt_str::format!("{} {hint}", provider.display_name),
                display_name: Str::from(provider.display_name),
                description,
                group: None,
            }
        })
        .collect();
    let picked = app_target::run_select(
        "Select a documentation provider (\u{2191}/\u{2193}, Enter to confirm):",
        "doc-provider-select",
        8,
        &items,
    )?;
    Ok(picked.map(|index| providers[index]))
}

/// The interactive no-provider flow for an action: offer to
/// initialize a provider, then continue with the requested command. Returns
/// an exit status when the command must stop: a declined prompt or a failed
/// initialization. Non-interactive sessions return `None` immediately; the
/// resolver then reports the standard error.
async fn offer_doc_init_for_action(
    cwd: &AbsolutePathBuf,
    options: Option<&CliOptions>,
) -> Result<Option<ExitStatus>, Error> {
    if !vp_shared::is_interactive_terminal() {
        return Ok(None);
    }
    // Config problems are reported by the resolver path, not here.
    let Ok(context) = resolver::load_doc_context(cwd.as_path(), options).await else {
        return Ok(None);
    };
    if !matches!(
        vp_doc_cli::select_provider(context.as_ref(), cwd.as_path()),
        Ok(vp_doc_cli::ProviderSelection::NoProvider)
    ) {
        return Ok(None);
    }
    println!("No documentation provider is configured.");
    let Some(provider) = run_doc_provider_picker()? else {
        return Ok(Some(ExitStatus(1)));
    };
    let status = execute_doc_init(Some(provider.id.to_string()), cwd, options, false).await?;
    if status.0 != 0 {
        return Ok(Some(status));
    }
    Ok(None)
}

/// Render a doc-context load failure: user messages print and exit 1, other
/// errors propagate.
fn doc_context_error(error: anyhow::Error) -> Result<ExitStatus, Error> {
    if let Some(vp_doc_cli::Error::UserMessage(message)) = error.downcast_ref() {
        eprintln!("error: {message}");
        return Ok(ExitStatus(1));
    }
    Err(Error::Anyhow(error))
}

/// Execute `vp doc init`: scaffold through `vp_doc_cli`, then install the
/// provider's dependencies through the normal package-manager dispatch.
async fn execute_doc_init(
    provider: Option<String>,
    cwd: &AbsolutePathBuf,
    options: Option<&CliOptions>,
    // False in the no-provider continue-flow, where the requested action
    // starts right after: a "run `vp doc`" hint there would name the very
    // command already running.
    print_next_step: bool,
) -> Result<ExitStatus, Error> {
    // An interactive session can omit the provider ID and pick from the
    // select prompt (rfcs/doc-command.md, Initialization).
    let mut provider = provider;
    if provider.is_none() && vp_shared::is_interactive_terminal() {
        let Some(picked) = run_doc_provider_picker()? else {
            return Ok(ExitStatus(1));
        };
        provider = Some(picked.id.to_string());
    }
    let outcome = match vp_doc_cli::init_scaffold(provider.as_deref(), cwd.as_path()) {
        Ok(outcome) => outcome,
        Err(error) => return render_doc_error(error),
    };

    match outcome {
        vp_doc_cli::DocInitOutcome::AlreadyConfigured { provider } => {
            println!(
                "{} is already set up (`{}` is declared).",
                provider.display_name, provider.marker
            );
            Ok(ExitStatus::SUCCESS)
        }
        vp_doc_cli::DocInitOutcome::Scaffolded { provider, files, dependencies } => {
            for file in &files {
                if file.created {
                    println!("Created {}.", file.path);
                } else {
                    println!("Kept existing {}.", file.path);
                }
            }
            println!("Installing {}...", dependencies.join(", "));

            let mut argv: Vec<String> = vec!["vp".into(), "add".into(), "-D".into()];
            argv.extend(dependencies.iter().map(|dep| (*dep).to_string()));
            let add = match CLIArgs::try_parse_from(&argv) {
                Ok(CLIArgs::PackageManager(command)) => command,
                _ => {
                    return Err(Error::Anyhow(anyhow::anyhow!(
                        "failed to build the dependency install command"
                    )));
                }
            };
            let status = execute_pm_command(add, cwd, options).await?;
            if status.0 != 0 {
                eprintln!("error: failed to install {}", dependencies.join(", "));
                return Ok(status);
            }

            // Step 3: write `doc` configuration only when detection alone
            // would not select the provider afterward
            // (rfcs/doc-command.md, Initialization).
            match vp_doc_cli::write_doc_provider_config(provider, cwd.as_path()) {
                Ok(vp_doc_cli::DocConfigWrite::NotNeeded) => {}
                Ok(vp_doc_cli::DocConfigWrite::Created) => {
                    println!("Created vite.config.ts with `doc.provider: '{}'`.", provider.id);
                }
                Ok(vp_doc_cli::DocConfigWrite::Updated { file }) => {
                    println!("Set `doc.provider: '{}'` in {file}.", provider.id);
                }
                Ok(vp_doc_cli::DocConfigWrite::Manual { file }) => {
                    println!(
                        "Another provider is declared. Set `doc.provider: '{}'` in {file} to select {}.",
                        provider.id, provider.display_name
                    );
                }
                Err(error) => return render_doc_error(error),
            }

            if print_next_step {
                println!(
                    "{} is ready. Run `vp doc` to start the dev server.",
                    provider.display_name
                );
            } else {
                println!("{} is ready.", provider.display_name);
            }
            Ok(ExitStatus::SUCCESS)
        }
    }
}

/// Execute `vp doc info`: report the resolved provider without starting the
/// tool.
async fn execute_doc_info(
    json: bool,
    cwd: &AbsolutePathBuf,
    options: Option<&CliOptions>,
) -> Result<ExitStatus, Error> {
    let context = match resolver::load_doc_context(cwd.as_path(), options).await {
        Ok(context) => context,
        Err(error) => return doc_context_error(error),
    };
    let report = match vp_doc_cli::info_report(cwd.as_path(), context.as_ref()) {
        Ok(report) => report,
        Err(error) => return render_doc_error(error),
    };
    if json {
        let rendered = serde_json::to_string_pretty(&report.to_json())
            .map_err(|error| Error::Anyhow(anyhow::Error::new(error)))?;
        println!("{rendered}");
        return Ok(if report.resolved() { ExitStatus::SUCCESS } else { ExitStatus(1) });
    }
    match &report {
        vp_doc_cli::DocInfoReport::Resolved(info) => {
            let provider = info.provider;
            println!("Provider:  {} ({})", provider.id, provider.display_name);
            match info.source {
                vp_doc_cli::SelectionSource::Config => {
                    println!("Source:    `doc.provider` in vite.config");
                }
                vp_doc_cli::SelectionSource::Marker => {
                    println!("Source:    dependency marker `{}` in package.json", provider.marker);
                }
            }
            let target = provider.target.as_str();
            let tool = info.execution_package();
            match info.execution_version.as_deref() {
                Some(version) => println!("Tool:      {tool}@{version} ({target})"),
                None => println!("Tool:      {tool} (not installed) ({target})"),
            }
            if info.marker_version.is_some() && !info.supported {
                match provider.version_range {
                    Some(range) => {
                        println!("Warning:   installed version is unsupported (requires `{range}`)")
                    }
                    None => println!("Warning:   installed version is unsupported"),
                }
            }
            let commands: Vec<&str> =
                provider.capabilities.iter().map(|action| action.as_str()).collect();
            println!("Commands:  {}", commands.join(", "));
        }
        vp_doc_cli::DocInfoReport::NoProvider => {
            println!(
                "No documentation provider is configured. Run `vp doc init <provider>` to set one up."
            );
        }
        vp_doc_cli::DocInfoReport::MultipleProviders { candidates } => {
            println!(
                "Multiple documentation providers are declared: {}. Remove the markers you do not use, or set `doc.provider` in vite.config.ts.",
                candidates.join(", ")
            );
        }
    }
    Ok(if report.resolved() { ExitStatus::SUCCESS } else { ExitStatus(1) })
}

/// Execute a package-manager command directly through `vp_pm_cli`,
/// bypassing the vite-task scheduler — PM operations don't need caching.
async fn execute_pm_command(
    command: vp_pm_cli::PackageManagerCommand,
    cwd: &AbsolutePath,
    options: Option<&CliOptions>,
) -> Result<ExitStatus, Error> {
    // Commands projected into the vite-plus-managed package store only work
    // in the global CLI. The local CLI has no such store, so refuse rather
    // than silently doing the wrong thing (mutating the project, dropping
    // `--node`, ignoring `--dry-run`, …).
    if command.is_managed_global() {
        return Err(Error::Anyhow(anyhow::anyhow!(
            "Global package operations (`-g`/`--global`) are only supported by the globally-installed `vp` CLI. See https://viteplus.dev/guide/ to install it, then run the same command via the global `vp` binary.",
        )));
    }
    let hint_command = command.clone();
    let result = match vp_pm_cli::dispatch_with_metadata(cwd, command).await {
        Ok(result) => result,
        // Render `UserMessage` cleanly (no `error:` prefix) and exit non-zero —
        // matches the global CLI's `is_user_message()` branch in main.rs so the
        // friendly version-gate / usage errors look the same on both surfaces.
        Err(vp_pm_cli::Error::UserMessage(msg)) => {
            vp_shared::output::raw_stderr(&msg);
            return Ok(ExitStatus(1));
        }
        Err(e) => return Err(Error::Anyhow(anyhow::Error::new(e))),
    };
    if result.status.success()
        && let Some(packages) = hint_command.why_hint_packages(result.package_manager)
    {
        print_toolchain_why_hint(options, packages);
    }
    Ok(types::exit_status_from(result.status))
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsStr,
        fs,
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use rustc_hash::FxHashMap;
    use vt::config::UserRunConfig;
    use vt_path::AbsolutePathBuf;

    use super::{Error, envs_with_explicit_package_manager_path, prepend_to_env_path};

    fn envs_with_path(path: &std::ffi::OsStr) -> Arc<FxHashMap<Arc<OsStr>, Arc<OsStr>>> {
        Arc::new(FxHashMap::from_iter([(Arc::from(OsStr::new("PATH")), Arc::from(path))]))
    }

    #[test]
    fn prepends_package_manager_bin_to_env_path() {
        let cwd = std::env::current_dir().expect("current_dir should exist");
        let old_bin = cwd.join("old-bin");
        let pm_bin = AbsolutePathBuf::new(cwd.join("pm-bin")).expect("pm bin should be absolute");
        let original_path = std::env::join_paths([old_bin.as_path()]).expect("valid PATH");
        let envs = envs_with_path(original_path.as_os_str());

        let updated = prepend_to_env_path(&envs, &pm_bin);
        let path_value = updated.get(OsStr::new("PATH")).expect("PATH should exist");
        let paths = std::env::split_paths(path_value).collect::<Vec<_>>();

        assert_eq!(paths.first().map(std::path::PathBuf::as_path), Some(pm_bin.as_path()));
        assert_eq!(paths.get(1).map(std::path::PathBuf::as_path), Some(old_bin.as_path()));
    }

    #[test]
    fn does_not_duplicate_package_manager_bin_when_already_first() {
        let cwd = std::env::current_dir().expect("current_dir should exist");
        let pm_bin = AbsolutePathBuf::new(cwd.join("pm-bin")).expect("pm bin should be absolute");
        let original_path = std::env::join_paths([pm_bin.as_path()]).expect("valid PATH");
        let envs = envs_with_path(original_path.as_os_str());

        let updated = prepend_to_env_path(&envs, &pm_bin);
        let path_value = updated.get(OsStr::new("PATH")).expect("PATH should exist");
        let paths = std::env::split_paths(path_value).collect::<Vec<_>>();

        assert_eq!(paths, vec![pm_bin.as_path().to_path_buf()]);
    }

    #[test]
    fn creates_path_when_env_map_has_no_path() {
        let cwd = std::env::current_dir().expect("current_dir should exist");
        let pm_bin = AbsolutePathBuf::new(cwd.join("pm-bin")).expect("pm bin should be absolute");
        let envs = Arc::new(FxHashMap::default());

        let updated = prepend_to_env_path(&envs, &pm_bin);
        let path_value = updated.get(OsStr::new("PATH")).expect("PATH should be created");
        let paths = std::env::split_paths(path_value).collect::<Vec<_>>();

        assert_eq!(paths, vec![pm_bin.as_path().to_path_buf()]);
    }

    #[test]
    fn preserves_path_key_casing_on_windows() {
        let cwd = std::env::current_dir().expect("current_dir should exist");
        let old_bin = cwd.join("old-bin");
        let pm_bin = AbsolutePathBuf::new(cwd.join("pm-bin")).expect("pm bin should be absolute");
        let original_path = std::env::join_paths([old_bin.as_path()]).expect("valid PATH");
        let key = if cfg!(windows) { "Path" } else { "PATH" };
        let envs = Arc::new(FxHashMap::from_iter([(
            Arc::from(OsStr::new(key)),
            Arc::from(original_path.as_os_str()),
        )]));

        let updated = prepend_to_env_path(&envs, &pm_bin);
        let path_value = updated.get(OsStr::new(key)).expect("existing PATH key should be updated");
        let paths = std::env::split_paths(path_value).collect::<Vec<_>>();

        assert_eq!(paths.first().map(std::path::PathBuf::as_path), Some(pm_bin.as_path()));
        assert_eq!(paths.get(1).map(std::path::PathBuf::as_path), Some(old_bin.as_path()));
    }

    #[tokio::test]
    async fn ignores_invalid_explicit_package_manager() {
        let suffix =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time should be valid").as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("vite-plus-invalid-pm-{suffix}"));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        fs::write(
            temp_dir.join("package.json"),
            r#"{"name":"fixture","packageManager":"unknown@1.0.0"}"#,
        )
        .expect("package.json should be written");
        let cwd = AbsolutePathBuf::new(temp_dir.clone()).expect("temp dir should be absolute");
        let original_path = std::env::join_paths([temp_dir.join("old-bin")]).expect("valid PATH");
        let envs = envs_with_path(original_path.as_os_str());

        let updated = envs_with_explicit_package_manager_path(&cwd, Arc::clone(&envs))
            .await
            .expect("package manager preflight errors should not fail direct commands");

        assert_eq!(updated.get(OsStr::new("PATH")), envs.get(OsStr::new("PATH")));
        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[tokio::test]
    async fn stops_when_the_pinned_package_manager_fails_its_integrity_check() {
        let suffix =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time should be valid").as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("vite-plus-bad-hash-{suffix}"));
        let vp_home = temp_dir.join("vp-home");
        let bin_dir =
            vp_home.join("package_manager").join("yarn").join("4.17.1").join("yarn").join("bin");
        fs::create_dir_all(&bin_dir).expect("cached package manager should be created");
        for shim in ["yarn", "yarn.cmd", "yarn.ps1"] {
            fs::write(bin_dir.join(shim), "shim").expect("shim should be written");
        }
        fs::write(bin_dir.join("yarn.js"), "corrupt").expect("CLI should be written");

        let expected_hash = format!("sha512.{}", "0".repeat(128));
        fs::write(
            temp_dir.join("package.json"),
            format!(r#"{{"name":"fixture","packageManager":"yarn@4.17.1+{expected_hash}"}}"#),
        )
        .expect("package.json should be written");
        let cwd = AbsolutePathBuf::new(temp_dir.clone()).expect("temp dir should be absolute");
        let original_path = std::env::join_paths([temp_dir.join("old-bin")]).expect("valid PATH");
        let envs = envs_with_path(original_path.as_os_str());

        let _guard =
            vp_shared::EnvConfig::test_guard(vp_shared::EnvConfig::for_test_with_home(&vp_home));
        let result = envs_with_explicit_package_manager_path(&cwd, envs).await;

        assert!(
            matches!(result, Err(Error::PackageManagerHashMismatch(_))),
            "an integrity failure must reach the user instead of a missing command: {result:?}"
        );
        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[tokio::test]
    async fn ignores_lockfile_without_explicit_package_manager() {
        let suffix =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time should be valid").as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("vite-plus-no-pm-{suffix}"));
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        fs::write(temp_dir.join("package.json"), r#"{"name":"fixture"}"#)
            .expect("package.json should be written");
        fs::write(temp_dir.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n")
            .expect("lockfile should be written");
        let cwd = AbsolutePathBuf::new(temp_dir.clone()).expect("temp dir should be absolute");
        let original_path = std::env::join_paths([temp_dir.join("old-bin")]).expect("valid PATH");
        let envs = envs_with_path(original_path.as_os_str());

        let updated = envs_with_explicit_package_manager_path(&cwd, Arc::clone(&envs))
            .await
            .expect("missing packageManager should not error");

        assert_eq!(updated.get(OsStr::new("PATH")), envs.get(OsStr::new("PATH")));
        assert_eq!(
            fs::read_to_string(temp_dir.join("package.json")).expect("package.json should exist"),
            r#"{"name":"fixture"}"#
        );
        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn run_config_types_in_sync() {
        // Remove \r for cross-platform consistency
        let ts_type = UserRunConfig::TS_TYPE.replace('\r', "");
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
        let run_config_path = PathBuf::from(manifest_dir).join("../src/run-config.ts");

        if std::env::var("VP_UPDATE_TASK_TYPES").as_deref() == Ok("1") {
            std::fs::write(&run_config_path, &ts_type).expect("Failed to write run-config.ts");
        } else {
            let current = std::fs::read_to_string(&run_config_path)
                .expect("Failed to read run-config.ts")
                .replace('\r', "");
            pretty_assertions::assert_eq!(
                current,
                ts_type,
                "run-config.ts is out of sync. Run `VP_UPDATE_TASK_TYPES=1 cargo test -p vite-plus-cli run_config_types_in_sync` to update."
            );
        }
    }
}
