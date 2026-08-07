//! Resolves and executes a parsed package-manager command.
//!
//! Callers must perform any environment setup (PATH adjustments, runtime
//! download) before invoking [`dispatch`].

use std::process::ExitStatus;

use vt_path::AbsolutePath;

use crate::{
    PackageManager,
    cli::{PackageManagerCommand, PmCommand},
    error::Error,
    helpers::{build_package_manager, build_package_manager_or_npm_default, ensure_package_json},
    resolution::{DlxArgs, StageCommand, run_resolution},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagerPolicy {
    CreateIfMissing,
    RequireProject,
    AllowNpmFallback,
}

pub async fn dispatch(
    cwd: &AbsolutePath,
    command: PackageManagerCommand,
) -> Result<ExitStatus, Error> {
    // POC (do not ship): route `vp install` through the embedded aube engine
    // when VP_INSTALL_ENGINE=aube. This path ignores install-command flags.
    if matches!(command, PackageManagerCommand::Install(_))
        && std::env::var_os("VP_INSTALL_ENGINE").is_some_and(|v| v == "aube")
    {
        return dispatch_aube_install(cwd).await;
    }

    let render_diagnostics = command.should_render_diagnostics();
    let command = match command {
        PackageManagerCommand::Dlx(args) => {
            return dispatch_dlx(cwd, args, render_diagnostics).await;
        }
        command => command,
    };

    let manager = match manager_policy(&command) {
        ManagerPolicy::CreateIfMissing => {
            ensure_package_json(cwd).await?;
            build_package_manager(cwd).await?
        }
        ManagerPolicy::RequireProject => build_package_manager(cwd).await?,
        ManagerPolicy::AllowNpmFallback => build_package_manager_or_npm_default(cwd).await?,
    };

    let resolution = command.resolve_for_manager(&manager)?;
    run_resolution(cwd, resolution, render_diagnostics).await
}

/// POC install path for the embedded aube engine. The host profile mirrors
/// the nub model: impersonate pnpm, read and write the project's lockfile.
async fn dispatch_aube_install(cwd: &AbsolutePath) -> Result<ExitStatus, Error> {
    static HOST: aube::embed::Host = aube::embed::Host {
        name: "vp",
        display_name: "Vite+",
        vendor: Some("VoidZero"),
        version: env!("CARGO_PKG_VERSION"),
        user_agent: concat!("vp/", env!("CARGO_PKG_VERSION")),
        self_names: &["vp"],
        compatible_names: &["pnpm"],
        lockfile_basename: "vp-lock.yaml",
        workspace_yaml: None,
        manifest_namespace: "vp",
        env_prefix: None,
        config_env_prefix: None,
        cache_namespace: "vp",
        data_namespace: "vp",
        canonical_lockfile_always_wins: false,
        runtime_switching: false,
        self_engines_check: false,
        self_update_enabled: false,
    };
    aube::embed::initialize(&HOST, Vec::new());

    let project_dir: &std::path::Path = cwd.as_ref();
    let options = aube::embed::InstallOptions::new(project_dir.to_path_buf());
    aube::embed::install(options)
        .await
        .map_err(|err| Error::UserMessage(format!("aube install failed: {err:#}").into()))?;

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        Ok(ExitStatus::from_raw(0))
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        Ok(ExitStatus::from_raw(0))
    }
}

async fn dispatch_dlx(
    cwd: &AbsolutePath,
    args: DlxArgs,
    render_diagnostics: bool,
) -> Result<ExitStatus, Error> {
    match PackageManager::builder(cwd).build_with_default().await {
        Ok(manager) => {
            let resolution = PackageManagerCommand::Dlx(args).resolve_for_manager(&manager)?;
            run_resolution(cwd, resolution, render_diagnostics).await
        }
        Err(vp_error::Error::WorkspaceError(vt_workspace::Error::PackageJsonNotFound(_))) => {
            run_resolution(cwd, args.resolve_npx_fallback(), render_diagnostics).await
        }
        Err(error) => Err(Error::Install(error)),
    }
}

fn manager_policy(command: &PackageManagerCommand) -> ManagerPolicy {
    match command {
        PackageManagerCommand::Install(_) | PackageManagerCommand::Add(_) => {
            ManagerPolicy::CreateIfMissing
        }
        PackageManagerCommand::Remove(_)
        | PackageManagerCommand::Update(_)
        | PackageManagerCommand::Dedupe(_)
        | PackageManagerCommand::Outdated(_)
        | PackageManagerCommand::Why(_)
        | PackageManagerCommand::Link(_)
        | PackageManagerCommand::Unlink(_) => ManagerPolicy::RequireProject,
        PackageManagerCommand::Info(_) => ManagerPolicy::AllowNpmFallback,
        PackageManagerCommand::Dlx(_) => {
            unreachable!("dlx commands are dispatched before manager policy selection")
        }
        PackageManagerCommand::Pm(command) => pm_manager_policy(command),
    }
}

fn pm_manager_policy(command: &PmCommand) -> ManagerPolicy {
    match command {
        PmCommand::Ci(_)
        | PmCommand::ApproveBuilds(_)
        | PmCommand::Prune(_)
        | PmCommand::Patch(_)
        | PmCommand::PatchCommit(_)
        | PmCommand::Pack(_)
        | PmCommand::List(_)
        | PmCommand::Version(_)
        | PmCommand::Publish(_)
        | PmCommand::Rebuild(_)
        | PmCommand::Fund(_)
        | PmCommand::Audit(_)
        | PmCommand::Stage(StageCommand::Publish { .. }) => ManagerPolicy::RequireProject,
        PmCommand::View(_)
        | PmCommand::Stage(_)
        | PmCommand::Owner(_)
        | PmCommand::Cache(_)
        | PmCommand::Config(_)
        | PmCommand::Login(_)
        | PmCommand::Logout(_)
        | PmCommand::Whoami(_)
        | PmCommand::Token(_)
        | PmCommand::DistTag(_)
        | PmCommand::Deprecate(_)
        | PmCommand::Search(_)
        | PmCommand::Ping(_) => ManagerPolicy::AllowNpmFallback,
    }
}

#[cfg(test)]
mod tests {
    use clap::{FromArgMatches, Subcommand};

    use super::*;

    fn parse_command(args: &[&str]) -> PackageManagerCommand {
        let mut command = PackageManagerCommand::augment_subcommands(clap::Command::new("vp"));
        let matches = command.try_get_matches_from_mut(args).unwrap();
        PackageManagerCommand::from_arg_matches(&matches).unwrap()
    }

    #[test]
    fn manager_policy_covers_project_creation_and_requirement() {
        assert_eq!(
            manager_policy(&parse_command(&["vp", "install"])),
            ManagerPolicy::CreateIfMissing
        );
        assert_eq!(
            manager_policy(&parse_command(&["vp", "remove", "react"])),
            ManagerPolicy::RequireProject
        );
    }

    #[test]
    fn manager_policy_covers_npm_fallbacks() {
        assert_eq!(
            manager_policy(&parse_command(&["vp", "info", "react"])),
            ManagerPolicy::AllowNpmFallback
        );
    }

    #[test]
    fn only_stage_publish_requires_a_project() {
        assert_eq!(
            manager_policy(&parse_command(&["vp", "pm", "stage", "publish"])),
            ManagerPolicy::RequireProject
        );
        assert_eq!(
            manager_policy(&parse_command(&["vp", "pm", "stage", "list"])),
            ManagerPolicy::AllowNpmFallback
        );
    }
}
