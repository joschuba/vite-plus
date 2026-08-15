//! Target elicitation for bare app commands at a workspace root.
//!
//! A bare `vp dev`/`build`/`preview`/`pack` at a workspace root has no target
//! and would silently run against the root. Resolution order (rfcs/cwd-flag.md):
//! explicit `-C` and positional targets are handled before this code and skip
//! elicitation entirely; then `defaultPackage` from the config in the
//! invocation directory, then the interactive package picker (a package
//! listing plus exit 1 when the terminal is not interactive).

use vp_error::Error;
use vp_shared::{env_vars, output};
use vt::ExitStatus;
use vt_path::{AbsolutePath, AbsolutePathBuf};
use vt_workspace::WorkspaceFile;

use super::types::SynthesizableSubcommand;

/// Where a bare app command should run.
pub(super) enum AppTarget {
    /// No elicitation applies; run in the invocation directory as today.
    CurrentDir,
    /// Run as if invoked in this directory (implicit `-C`).
    Dir(AbsolutePathBuf),
    /// Elicitation printed its output and decided the exit code.
    Exit(ExitStatus),
}

struct PackageRow {
    name: vt_str::Str,
    path: vt_str::Str,
    absolute: AbsolutePathBuf,
    runnable: bool,
}

/// App commands are the single-target subcommands; everything else never
/// goes through elicitation.
fn app_command_parts(subcommand: &SynthesizableSubcommand) -> Option<(&'static str, &[String])> {
    match subcommand {
        SynthesizableSubcommand::Dev { args } => Some(("dev", args)),
        SynthesizableSubcommand::Build { args } => Some(("build", args)),
        SynthesizableSubcommand::Preview { args } => Some(("preview", args)),
        SynthesizableSubcommand::Pack { args } => Some(("pack", args)),
        _ => None,
    }
}

/// Boolean flags of the Vite CLI (dev/build/preview), from the shipped
/// `vp <command> --help` (snap-tests/command-helper); keep in sync. Under
/// cac/mri parsing every OTHER flag — required-value, optional-value
/// (`--host [host]`), or unknown — consumes a following non-flag token as
/// its value, so only tokens no flag consumes are positional targets.
const VITE_BOOLEAN_FLAGS: &[&str] = &[
    "-w",
    "--watch",
    "--app",
    "--clearScreen",
    "--cors",
    "--emptyOutDir",
    "--experimentalBundle",
    "--force",
    "--profile",
    "--strictPort",
];

/// Boolean flags of the bundled pack CLI (tsdown), from `vp pack --help`.
const PACK_BOOLEAN_FLAGS: &[&str] = &[
    "--attw",
    "--clean",
    "--devtools",
    "--dts",
    "--exe",
    "--exports",
    "--fail-on-warn",
    "--failOnWarn",
    "--minify",
    "--no-write",
    "--publint",
    "--report",
    "--shims",
    "--sourcemap",
    "--treeshake",
    "--unbundle",
    "--unused",
];

/// How an app command's arguments target it, per the walk in
/// [`classify_args`].
enum ArgTarget<'a> {
    /// No positional target and no help-like flag: elicitation territory.
    Bare,
    /// The first token the tool would treat as a positional (a Vite `[root]`
    /// or a pack entry), including one after a `--` terminator.
    Positional(&'a str),
    /// Explicitly targeted without a positional (help/version request, an
    /// explicit `-c`/`--config` file, pack workspace selectors): forward
    /// untouched.
    Explicit,
}

/// Bare = no positional target and no help-like flag.
fn is_bare(command: &str, args: &[String]) -> bool {
    matches!(classify_args(command, args), ArgTarget::Bare)
}

/// Mirrors the tools' own cac/mri parsing: a non-flag token after any
/// non-boolean flag is that flag's value (the tool would never see it as a
/// positional), while a token after a boolean flag is a positional target
/// and disables elicitation. pack's workspace selectors already define their
/// own target set and disable elicitation outright. Help/version requests
/// are answered by the underlying tool and must never be redirected.
fn classify_args<'a>(command: &str, args: &'a [String]) -> ArgTarget<'a> {
    /// `arg` is one of `flags`, exactly or in inline `flag=value` form.
    fn matches_flag(arg: &str, flags: &[&str]) -> bool {
        flags.iter().any(|f| arg == *f || arg.strip_prefix(f).is_some_and(|r| r.starts_with('=')))
    }

    let is_pack = command == "pack";
    let booleans = if is_pack { PACK_BOOLEAN_FLAGS } else { VITE_BOOLEAN_FLAGS };
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if !arg.starts_with('-') {
            return ArgTarget::Positional(arg);
        }
        if super::help::is_app_tool_help_or_version_flag(arg) {
            return ArgTarget::Explicit;
        }
        // `--` terminates options: whatever follows is an explicit positional.
        if arg == "--" {
            return match iter.next() {
                Some(token) => ArgTarget::Positional(token),
                None => ArgTarget::Bare,
            };
        }
        // An explicit config file (`-c`/`--config`) is explicit build intent:
        // forward it to the tool instead of eliciting a package to override it.
        if matches_flag(arg, &["-c", "--config"]) {
            return ArgTarget::Explicit;
        }
        // Workspace selectors and --root already specify pack's target;
        // these previously-valid targeted invocations must keep forwarding.
        if is_pack && matches_flag(arg, &["-W", "--workspace", "-F", "--filter", "--root"]) {
            return ArgTarget::Explicit;
        }
        let is_boolean = booleans.contains(&arg.as_str()) || arg.starts_with("--no-");
        // An inline `=` already carries the value (`--port=3000`, `--env.FOO=bar`).
        if !is_boolean
            && !arg.contains('=')
            && iter.peek().is_some_and(|next| !next.starts_with('-'))
        {
            iter.next();
        }
    }
    ArgTarget::Bare
}

/// Heuristic ranking signal: does a directory look runnable for `command`?
/// Used for ordering and single-candidate auto-selection, never for hiding.
/// The rules are documented in rfcs/cwd-flag.md ("The likely-runnable
/// heuristic"); keep both in sync. Two variants because the workspace root
/// needs a stronger signal than member packages: a shared root
/// `vite.config.ts` (lint/fmt/tasks) is the normal monorepo setup and must
/// not make the root look like an app, or auto-select would run the silent
/// root build this feature exists to prevent.
///
/// Root variant. Takes the root config [`classify`] already resolved for the
/// `defaultPackage` lookup, so the file is read and parsed once per
/// invocation. A declared `build` block (a library/SSR build with no entry
/// HTML) makes the root a target for `vp build` only: dev/preview serve an
/// app, for which the signal is a root `index.html`. A shared root config
/// for lint/fmt/tasks declares neither, so it never makes the root a target.
fn root_looks_runnable(
    config: &vp_static_config::FieldMap,
    dir: &AbsolutePath,
    command: &str,
) -> bool {
    match command {
        // Bare `vp pack` succeeds when tsdown's default entry exists or the
        // config explicitly declares a `pack` block (a spread that only
        // might contain `pack` does not count: auto-select acts on this
        // signal, so a false positive runs tsdown in a non-packable package).
        "pack" => {
            dir.as_path().join("src/index.ts").is_file() || config.get_declared("pack").is_some()
        }
        "build" => {
            dir.as_path().join("index.html").is_file() || config.get_declared("build").is_some()
        }
        _ => dir.as_path().join("index.html").is_file(),
    }
}

/// Member variant of the likely-runnable heuristic; see
/// [`root_looks_runnable`]. Resolves the member's own config lazily: this
/// executes per workspace package, and for `pack` the one-stat entry check
/// runs first because the config check reads and parses a file.
fn member_looks_runnable(dir: &AbsolutePath, command: &str) -> bool {
    match command {
        "pack" => {
            dir.as_path().join("src/index.ts").is_file()
                || vp_static_config::resolve_static_config(dir).get_declared("pack").is_some()
        }
        _ => vp_static_config::has_config_file(dir) || dir.as_path().join("index.html").is_file(),
    }
}

/// Resolve the `defaultPackage` value [`classify`] extracted from the
/// invocation root's `vite.config.*` (static extraction, so it works at
/// roots without a vite-plus install). The value must be a static string
/// literal naming an existing directory.
fn resolve_default_package(
    command: &str,
    cwd: &AbsolutePath,
    value: vp_static_config::FieldValue,
) -> AppTarget {
    let fail = |msg: &str| {
        output::error(msg);
        AppTarget::Exit(ExitStatus(1))
    };
    match value {
        vp_static_config::FieldValue::Json(serde_json::Value::String(dir)) => {
            let target = cwd.join(&dir).clean();
            if !target.as_path().is_dir() {
                return fail(&format!("defaultPackage points to a missing directory: {dir}"));
            }
            output::note(&format!("vp {command}: using {dir} (defaultPackage in vite.config.ts)"));
            AppTarget::Dir(target)
        }
        vp_static_config::FieldValue::Json(other) => {
            fail(&format!("defaultPackage must be a string of a directory, got: {other}"))
        }
        vp_static_config::FieldValue::NonStatic => fail(
            "defaultPackage in vite.config.ts must be a static string literal so vp can read it without executing the config",
        ),
    }
}

/// A workspace package that declares a documentation provider marker.
struct DocCandidateRow {
    name: vt_str::Str,
    path: vt_str::Str,
    absolute: AbsolutePathBuf,
    providers: String,
}

/// The doc-specific classification, resolved with one workspace walk and
/// one static-config read per invocation (rfcs/doc-command.md).
enum DocClassification {
    /// No redirect and no candidates: the command runs in place.
    RunInPlace,
    /// The `defaultPackage` `doc` entry applies (the carried value may be
    /// invalid or non-static; [`resolve_default_package`] reports it).
    Redirect(vp_static_config::FieldValue),
    /// Marker-declaring members at a real workspace root without a root
    /// marker: picker or listing territory for the actions.
    Elicit(Vec<DocCandidateRow>),
}

/// Classify a `vp doc` invocation directory. Pure: never prints.
fn classify_doc(cwd: &AbsolutePath) -> Result<DocClassification, Error> {
    let workspace = vt_workspace::find_workspace_root(cwd);
    let at_invocation_root =
        workspace.as_ref().map_or(true, |(_, rel_from_root)| rel_from_root.as_str().is_empty());
    if !at_invocation_root {
        return Ok(DocClassification::RunInPlace);
    }
    // The `doc` entry of `defaultPackage` redirects every doc subcommand.
    // Only the object form carries it: the string form covers the app
    // commands and never `doc`. Any other declared shape passes through so
    // the redirect reports it loudly, like the app commands.
    match vp_static_config::resolve_static_config(cwd).get_declared("defaultPackage") {
        Some(vp_static_config::FieldValue::Json(serde_json::Value::Object(map))) => {
            if let Some(value) = map.get("doc") {
                return Ok(DocClassification::Redirect(vp_static_config::FieldValue::Json(
                    value.clone(),
                )));
            }
        }
        Some(vp_static_config::FieldValue::Json(serde_json::Value::String(_))) | None => {}
        Some(invalid) => return Ok(DocClassification::Redirect(invalid)),
    }
    // The workspace documentation-package elicitation (rfcs/doc-command.md,
    // Monorepos). Anything unresolvable keeps today's behavior.
    let Ok((workspace_root, _)) = workspace else {
        return Ok(DocClassification::RunInPlace);
    };
    if matches!(workspace_root.workspace_file, WorkspaceFile::NonWorkspacePackage(_)) {
        return Ok(DocClassification::RunInPlace);
    }
    // A workspace root that declares its own marker is its own
    // documentation site (the root-VitePress layout); detection handles it.
    if !vp_doc_cli::detect_providers_in_dir(workspace_root.path.as_path()).is_empty() {
        return Ok(DocClassification::RunInPlace);
    }
    let graph =
        vt_workspace::load_package_graph(&workspace_root).map_err(|e| Error::Anyhow(e.into()))?;
    let mut rows: Vec<DocCandidateRow> = graph
        .node_weights()
        .filter(|info| !info.path.as_str().is_empty())
        .filter_map(|info| {
            // The graph already parsed each member manifest; check the
            // markers against its declared-dependency maps directly
            // (`peerDependencies` stays excluded, its own field there).
            let providers = vp_doc_cli::detect_providers_by(|marker| {
                info.package_json.dependencies.contains_key(marker)
                    || info.package_json.dev_dependencies.contains_key(marker)
            });
            if providers.is_empty() {
                return None;
            }
            Some(DocCandidateRow {
                name: info.package_json.name.clone(),
                path: vt_str::Str::from(info.path.as_str()),
                absolute: info.absolute_path.to_absolute_path_buf(),
                providers: providers.iter().map(|p| p.id).collect::<Vec<_>>().join(", "),
            })
        })
        .collect();
    if rows.is_empty() {
        return Ok(DocClassification::RunInPlace);
    }
    rows.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()));
    Ok(DocClassification::Elicit(rows))
}

/// Resolve a `vp doc` invocation directory (rfcs/doc-command.md): the
/// `defaultPackage` `doc` entry behaves as an implicit `-C` for every doc
/// subcommand; the documentation-package picker/listing applies to the
/// actions only (`elicit` is false for `init`/`info`). `command` is the
/// invocation to echo in hints (`doc`, `doc build`, `doc preview`).
pub(super) fn resolve_doc_target(
    cwd: &AbsolutePath,
    command: &str,
    elicit: bool,
) -> Result<AppTarget, Error> {
    match classify_doc(cwd)? {
        DocClassification::RunInPlace => Ok(AppTarget::CurrentDir),
        DocClassification::Redirect(value) => Ok(resolve_default_package("doc", cwd, value)),
        DocClassification::Elicit(_) if !elicit => Ok(AppTarget::CurrentDir),
        DocClassification::Elicit(rows) => elicit_doc_package(&rows, command),
    }
}

/// The picker or listing over the documentation-package candidates: the
/// interactive picker filtered to marker-declaring packages, or the
/// non-interactive candidate listing with exit 1.
fn elicit_doc_package(rows: &[DocCandidateRow], command: &str) -> Result<AppTarget, Error> {
    if vp_shared::is_interactive_terminal() {
        let items: Vec<vt_select::SelectItem> = rows
            .iter()
            .map(|row| vt_select::SelectItem {
                label: vt_str::format!("{} {}", row.name, row.path),
                display_name: row.name.clone(),
                description: row.path.clone(),
                group: None,
            })
            .collect();
        let prompt =
            "Select a documentation package (\u{2191}/\u{2193}, Enter to run, type to search):";
        let Some(index) = run_select(prompt, "doc-package-select", 12, &items)? else {
            return Ok(AppTarget::Exit(ExitStatus(130)));
        };
        let row = &rows[index];
        announce_selection(&row.name, &row.path, command);
        return Ok(AppTarget::Dir(row.absolute.clone()));
    }

    let header = if rows.len() == 1 {
        "a workspace package declares a documentation provider"
    } else {
        "several workspace packages declare a documentation provider"
    };
    output::error(header);
    output::raw_stderr("");
    let invocations: Vec<String> =
        rows.iter().map(|row| format!("vp -C {} {command}", row.path)).collect();
    let width = invocations.iter().map(String::len).max().unwrap_or(0);
    for (invocation, row) in invocations.iter().zip(rows) {
        output::raw_stderr(&format!("  {invocation:<width$}  ({})", row.providers));
    }
    Ok(AppTarget::Exit(ExitStatus(1)))
}

/// Fuzzy package picker on `vt_select`, the same component behind the
/// `vp run` task selector. Returns the selected row index, or `None` on
/// Ctrl+C.
fn run_package_picker(command: &str, rows: &[PackageRow]) -> Result<Option<usize>, Error> {
    let items: Vec<vt_select::SelectItem> = rows
        .iter()
        .map(|row| vt_select::SelectItem {
            label: vt_str::format!("{} {}", row.name, row.path),
            display_name: row.name.clone(),
            description: row.path.clone(),
            group: None,
        })
        .collect();
    let prompt =
        format!("Select a package to {command} (\u{2191}/\u{2193}, Enter to run, type to search):");
    run_select(&prompt, "package-select", 12, &items)
}

/// The one `vt_select` wrapper behind every picker here: the
/// `VP_EMIT_MILESTONES` gate, the `<prefix>:<query>:<index>` render
/// milestone, and the Selected/Cancelled mapping live in one place so the
/// pickers and the PTY snapshot protocol cannot drift apart. When the
/// runner sets `VP_EMIT_MILESTONES=1`, every render emits the milestone as
/// an invisible window-title update (same gate and protocol as
/// packages/prompts/src/milestone.ts); real terminals never see it.
pub(super) fn run_select(
    prompt: &str,
    milestone_prefix: &str,
    page_size: usize,
    items: &[vt_select::SelectItem],
) -> Result<Option<usize>, Error> {
    let emit_milestones =
        std::env::var_os(env_vars::VP_EMIT_MILESTONES).is_some_and(|value| value == "1");
    let params = vt_select::SelectParams { items, query: None, header: None, prompt, page_size };
    let mut selected_index = 0usize;
    let mut stdout = std::io::stdout();
    let result = vt_select::select_list(
        &mut stdout,
        &params,
        vt_select::Mode::Interactive { selected_index: &mut selected_index },
        |state| {
            if !emit_milestones {
                return;
            }
            let milestone =
                vt_str::format!("{milestone_prefix}:{}:{}", state.query, state.selected_index);
            emit_milestone_title(&milestone);
        },
    )
    .map_err(Error::Anyhow)?;
    Ok(match result {
        vt_select::SelectResult::Selected => Some(selected_index),
        vt_select::SelectResult::Cancelled => None,
    })
}

/// The picker epilogue, deliberately stdout via `println!`: these lines
/// belong to the command's own output stream, like the tool output that
/// follows.
fn announce_selection(name: &str, path: &str, command: &str) {
    println!("Selected package: {name} ({path})");
    println!("Tip: run this directly with `vp -C {path} {command}`");
}

/// Emits a render-milestone as a window-title update for the PTY snapshot
/// runner, mirroring packages/prompts/src/milestone.ts:
/// `OSC 2 ; pty-terminal-test:<32-hex-id>:<base64url(name)> ST`. The protocol
/// is shared with vite-task's `pty_terminal_test_client`, whose emitting API
/// compiles to a no-op outside its `testing` feature (enabling that feature
/// here would also un-gate vt's own task-picker milestones in
/// production), so the sequence is written by hand. A fresh random id per
/// emission keeps repeated milestones with the same name observable as
/// distinct title changes through Windows ConPTY.
pub(super) fn emit_milestone_title(name: &str) {
    use std::io::Write as _;
    let id = uuid::Uuid::new_v4();
    let encoded_name = base64_simd::URL_SAFE_NO_PAD.encode_to_string(name.as_bytes());
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "\x1b]2;pty-terminal-test:{}:{encoded_name}\x1b\\", id.simple());
    let _ = out.flush();
}

/// Pure predicate for the vp-script interception: would target resolution
/// do anything other than run in `cwd`? Never prints and never runs the
/// picker. Slightly over-approximates (an empty workspace reports true), in
/// which case the script merely spawns the real binary, which then behaves
/// identically to a direct invocation.
pub(super) fn needs_elicitation(subcommand: &SynthesizableSubcommand, cwd: &AbsolutePath) -> bool {
    // Doc scripts: only `dev`, `build`, and `preview` synthesize — `init`,
    // `info`, and parse errors spawn the real binary — and the
    // `defaultPackage` `doc` redirect or the documentation-package
    // elicitation spawns it too (rfcs/doc-command.md, Task Runner and
    // Caching). A classification error counts as no elicitation; the
    // synthesized command then surfaces its own error.
    if let SynthesizableSubcommand::Doc { args } = subcommand {
        return !matches!(
            vp_doc_cli::parse_doc_args(args),
            Ok(vp_doc_cli::DocInvocation::Action(_))
        ) || matches!(
            classify_doc(cwd),
            Ok(DocClassification::Redirect(_) | DocClassification::Elicit(_))
        );
    }
    matches!(classify(subcommand, cwd), Classification::Elicit(..))
}

/// Outcome of classifying a bare app command.
enum Classification {
    /// Run in `cwd` unchanged. Carries the workspace root found for `cwd`
    /// (when the lookup succeeded) so the caller can reuse it instead of
    /// walking the tree a second time — the hot path for a bare command deep
    /// inside a large monorepo, where the walk is the only per-invocation
    /// cost this feature adds.
    RunInPlace(Option<vt_workspace::WorkspaceRoot>),
    /// Elicit a target: `defaultPackage`, or the picker/listing at a
    /// workspace root.
    Elicit(&'static str, Elicitation),
}

/// Why a bare app command needs target elicitation.
enum Elicitation {
    /// The invocation root's config explicitly declares `defaultPackage`
    /// (with this value — possibly invalid, which the resolver reports).
    DefaultPackage(vp_static_config::FieldValue),
    /// Bare app command at a real workspace root: picker/listing territory.
    WorkspaceRoot(vt_workspace::WorkspaceRoot),
}

/// Applies a `defaultPackage` declaration to one command. A string covers
/// all four app commands; an object maps commands individually
/// (`{ dev: './apps/web', pack: './packages/ui' }`), and a command absent
/// from the object falls through to the picker/listing resolution. Every
/// other shape (a non-string, a non-static value) passes through for
/// [`resolve_default_package`] to report.
fn default_package_for_command(
    command: &str,
    value: vp_static_config::FieldValue,
) -> Option<vp_static_config::FieldValue> {
    match value {
        vp_static_config::FieldValue::Json(serde_json::Value::Object(map)) => {
            map.get(command).cloned().map(vp_static_config::FieldValue::Json)
        }
        other => Some(other),
    }
}

/// The RFC's resolution order, written once for both entry points: bare app
/// command, then `defaultPackage` at the invocation root, then the workspace
/// root itself. `defaultPackage` is a root-pointer concept: it applies where
/// the invocation directory is its own root (a workspace root, a standalone
/// package, or a framework directory with no package.json ancestry); below a
/// workspace root the current directory already identifies the target, so a
/// member's own config must not redirect.
///
/// The one `find_workspace_root` walk here rides back out on
/// [`Classification::RunInPlace`] whenever the command ends up running in
/// `cwd`, so a bare command in a sub-package walks the tree once, not twice.
fn classify(subcommand: &SynthesizableSubcommand, cwd: &AbsolutePath) -> Classification {
    let Some((command, args)) = app_command_parts(subcommand) else {
        return Classification::RunInPlace(None);
    };
    if !is_bare(command, args) {
        return Classification::RunInPlace(None);
    }
    let workspace = vt_workspace::find_workspace_root(cwd);
    let at_invocation_root =
        workspace.as_ref().map_or(true, |(_, rel_from_root)| rel_from_root.as_str().is_empty());
    // Resolved once and reused by `root_looks_runnable` below, so a bare
    // command at a root reads and parses the config a single time.
    let root_config = at_invocation_root.then(|| vp_static_config::resolve_static_config(cwd));
    if let Some(value) = root_config
        .as_ref()
        .and_then(|config| config.get_declared("defaultPackage"))
        .and_then(|value| default_package_for_command(command, value))
    {
        return Classification::Elicit(command, Elicitation::DefaultPackage(value));
    }
    // The picker/listing needs workspace metadata; anything unresolvable
    // keeps today's behavior (the caller surfaces its own workspace errors).
    let Ok((workspace_root, rel_from_root)) = workspace else {
        return Classification::RunInPlace(None);
    };
    if !rel_from_root.as_str().is_empty()
        || matches!(workspace_root.workspace_file, WorkspaceFile::NonWorkspacePackage(_))
    {
        return Classification::RunInPlace(Some(workspace_root));
    }
    // A runnable workspace root runs in place, TTY or not: the invocation
    // already has its configured target, and repos whose root is the app or
    // library (e.g. a single package with a settings-only pnpm-workspace.yaml)
    // ran this way before elicitation existed. Eliciting only when the root
    // is not a plausible target is what keeps this feature purely additive.
    // An empty `rel_from_root` means the invocation is at the root, so the
    // config resolved above is present; degrade to running in place rather
    // than panic if that invariant ever breaks.
    let Some(root_config) = root_config else {
        return Classification::RunInPlace(Some(workspace_root));
    };
    if root_looks_runnable(&root_config, &workspace_root.path, command) {
        return Classification::RunInPlace(Some(workspace_root));
    }
    Classification::Elicit(command, Elicitation::WorkspaceRoot(workspace_root))
}

/// One-line guidance when a dev/build/preview positional names a directory:
/// the positional keeps upstream Vite semantics (`root` only, cwd untouched),
/// which diverges from `-C` exactly when the target is a directory, so this
/// is the moment to teach the `cd`-equivalent form. pack positionals are
/// tsdown entry files and never directories, so pack is excluded. Direct
/// invocations only: the task-script interception path never reaches
/// [`resolve_app_target`], keeping task output clean.
fn note_directory_positional(subcommand: &SynthesizableSubcommand, cwd: &AbsolutePath) {
    let Some((command, args)) = app_command_parts(subcommand) else { return };
    if command == "pack" {
        return;
    }
    if let ArgTarget::Positional(target) = classify_args(command, args)
        && cwd.join(target).clean().as_path().is_dir()
    {
        output::note(&format!(
            "`vp {command} {target}` sets Vite's root without changing the working directory. \
             To run as if started there, use `vp -C {target} {command}`."
        ));
    }
}

/// Resolve a bare app command's target. The second tuple element is the
/// workspace root already found for `cwd`, present only when the command runs
/// in the unchanged `cwd` (so it always matches a fresh lookup there); the
/// caller reuses it to skip a second `find_workspace_root` walk.
pub(super) fn resolve_app_target(
    subcommand: &SynthesizableSubcommand,
    cwd: &AbsolutePath,
) -> Result<(AppTarget, Option<vt_workspace::WorkspaceRoot>), Error> {
    note_directory_positional(subcommand, cwd);
    let (command, elicitation) = match classify(subcommand, cwd) {
        Classification::RunInPlace(workspace_root) => {
            return Ok((AppTarget::CurrentDir, workspace_root));
        }
        Classification::Elicit(command, elicitation) => (command, elicitation),
    };
    let workspace_root = match elicitation {
        Elicitation::DefaultPackage(value) => {
            return Ok((resolve_default_package(command, cwd, value), None));
        }
        Elicitation::WorkspaceRoot(workspace_root) => workspace_root,
    };

    let graph =
        vt_workspace::load_package_graph(&workspace_root).map_err(|e| Error::Anyhow(e.into()))?;
    let mut rows: Vec<PackageRow> = graph
        .node_weights()
        .filter(|info| {
            // The root is never a row: when it looks runnable, classify
            // already ran the command in place instead of eliciting.
            !info.path.as_str().is_empty()
        })
        .map(|info| {
            let absolute = info.absolute_path.to_absolute_path_buf();
            PackageRow {
                name: info.package_json.name.clone(),
                path: vt_str::Str::from(info.path.as_str()),
                runnable: member_looks_runnable(&absolute, command),
                absolute,
            }
        })
        .collect();
    if rows.is_empty() {
        // Root excluded and no members: runs in place, and the root we found
        // is still valid for the unchanged cwd.
        return Ok((AppTarget::CurrentDir, Some(workspace_root)));
    }
    rows.sort_by(|a, b| (!a.runnable, a.path.as_str()).cmp(&(!b.runnable, b.path.as_str())));

    // In an interactive terminal, pick the target: exactly one likely-runnable
    // package (rows are sorted runnable first) auto-selects without a menu;
    // otherwise the fuzzy picker runs.
    if vp_shared::is_interactive_terminal() {
        let single_runnable = rows[0].runnable && rows.get(1).is_none_or(|row| !row.runnable);
        let picked = if single_runnable { Some(0) } else { run_package_picker(command, &rows)? };
        let Some(index) = picked else {
            return Ok((AppTarget::Exit(ExitStatus(130)), None));
        };
        let row = &rows[index];
        announce_selection(&row.name, &row.path, command);
        return Ok((AppTarget::Dir(row.absolute.clone()), None));
    }

    output::error(&format!("`vp {command}` at the workspace root needs a target package."));
    output::raw_stderr("");
    output::raw_stderr("  Packages in this workspace:");
    let name_width = rows.iter().map(|row| row.name.len()).max().unwrap_or(0);
    for row in &rows {
        output::raw_stderr(&format!("    {:<name_width$}  {}", row.name, row.path));
    }
    output::raw_stderr("");
    let example = &rows[0].path;
    output::raw_stderr(&format!("  Pass a directory:  vp -C {example} {command}"));
    output::raw_stderr(&format!("  Or run every package's {command} script:  vp run -r {command}"));
    Ok((AppTarget::Exit(ExitStatus(1)), None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_means_no_positional_target_and_no_help() {
        let to_args = |args: &[&str]| args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert!(is_bare("dev", &to_args(&[])));
        assert!(is_bare("dev", &to_args(&["--watch"])));
        assert!(is_bare("build", &to_args(&["-w", "--minify"])));
        // A positional target disables elicitation.
        assert!(!is_bare("dev", &to_args(&["apps/web"])));
        // Like cac, any non-boolean flag consumes a following non-flag token
        // as its value — required and optional values alike.
        assert!(is_bare("dev", &to_args(&["--port", "3000"])));
        assert!(is_bare("dev", &to_args(&["--host", "0.0.0.0"])));
        assert!(is_bare("dev", &to_args(&["--open", "/foo"])));
        assert!(is_bare("build", &to_args(&["--mode", "production", "--minify"])));
        assert!(is_bare("build", &to_args(&["--port=3000"])));
        assert!(is_bare("pack", &to_args(&["--env-file", ".env"])));
        assert!(is_bare("pack", &to_args(&["--env.FOO=bar", "--minify"])));
        // A token after a boolean flag is a positional; the tables are
        // command-specific (--minify is optional-value for Vite build,
        // boolean for pack).
        assert!(!is_bare("build", &to_args(&["--watch", "apps/web"])));
        assert!(!is_bare("pack", &to_args(&["--minify", "src/index.ts"])));
        assert!(!is_bare("pack", &to_args(&["--env.FOO", "bar", "src/cli.ts"])));
        assert!(is_bare("build", &to_args(&["--minify", "esbuild"])));
        // pack workspace selectors define their own target set, in both the
        // spaced and inline-value forms.
        assert!(!is_bare("pack", &to_args(&["-W"])));
        assert!(!is_bare("pack", &to_args(&["--workspace", "packages/a"])));
        assert!(!is_bare("pack", &to_args(&["-F", "ui"])));
        assert!(!is_bare("pack", &to_args(&["--filter=ui"])));
        assert!(!is_bare("pack", &to_args(&["--workspace=packages/a"])));
        assert!(!is_bare("pack", &to_args(&["--root", "packages/lib"])));
        assert!(!is_bare("pack", &to_args(&["--root=packages/lib"])));
        // An explicit config file is an explicit target (build and pack).
        assert!(!is_bare("build", &to_args(&["-c", "apps/web/vite.config.ts"])));
        assert!(!is_bare("build", &to_args(&["--config", "apps/web/vite.config.ts"])));
        assert!(!is_bare("build", &to_args(&["--config=apps/web/vite.config.ts"])));
        assert!(!is_bare("preview", &to_args(&["-c", "x.ts"])));
        assert!(!is_bare("pack", &to_args(&["-c", "tsdown.config.ts"])));
        // `--` terminates options; a token after it is an explicit positional.
        assert!(!is_bare("build", &to_args(&["--", "apps/web"])));
        assert!(!is_bare("pack", &to_args(&["--minify", "--", "src/index.ts"])));
        assert!(is_bare("build", &to_args(&["--"])));
        // Help/version requests go to the underlying tool, never elicitation.
        assert!(!is_bare("dev", &to_args(&["--help"])));
        assert!(!is_bare("dev", &to_args(&["-h"])));
        assert!(!is_bare("build", &to_args(&["--watch", "--version"])));
        // Vite and tsdown are cac-based and use `-v` for version.
        assert!(!is_bare("build", &to_args(&["-v"])));
    }

    #[test]
    fn classify_args_reports_the_positional_token() {
        let to_args = |args: &[&str]| args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let positional = |command: &str, args: &[&str]| match classify_args(command, &to_args(args))
        {
            ArgTarget::Positional(token) => Some(token.to_string()),
            _ => None,
        };
        assert_eq!(positional("dev", &["apps/web"]), Some("apps/web".to_string()));
        assert_eq!(positional("build", &["--watch", "apps/web"]), Some("apps/web".to_string()));
        assert_eq!(positional("build", &["--", "apps/web"]), Some("apps/web".to_string()));
        // A value-consuming flag swallows the token: not a positional.
        assert_eq!(positional("dev", &["--port", "3000"]), None);
        // Help and explicit-config invocations are Explicit, not positional.
        assert!(matches!(classify_args("dev", &to_args(&["--help"])), ArgTarget::Explicit));
        assert!(matches!(classify_args("build", &to_args(&["-c", "x.ts"])), ArgTarget::Explicit));
    }

    #[test]
    fn only_app_commands_elicit() {
        for (subcommand, expected) in [
            (SynthesizableSubcommand::Dev { args: vec![] }, Some("dev")),
            (SynthesizableSubcommand::Build { args: vec![] }, Some("build")),
            (SynthesizableSubcommand::Preview { args: vec![] }, Some("preview")),
            (SynthesizableSubcommand::Pack { args: vec![] }, Some("pack")),
            (SynthesizableSubcommand::Lint { args: vec![] }, None),
            (SynthesizableSubcommand::Test { args: vec![] }, None),
        ] {
            assert_eq!(app_command_parts(&subcommand).map(|(name, _)| name), expected);
        }
    }

    /// A fresh directory under the OS temp dir; the caller removes it.
    fn doc_temp_dir(name: &str) -> (std::path::PathBuf, AbsolutePathBuf) {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should be valid")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vite-plus-doc-target-{name}-{suffix}"));
        std::fs::create_dir_all(&dir).expect("temp dir should be created");
        let abs = AbsolutePathBuf::new(dir.clone()).expect("temp dir should be absolute");
        (dir, abs)
    }

    fn doc_subcommand(args: &[&str]) -> SynthesizableSubcommand {
        SynthesizableSubcommand::Doc { args: args.iter().map(|arg| (*arg).to_string()).collect() }
    }

    #[test]
    fn doc_redirect_applies_the_object_entry_to_every_subcommand() {
        let (dir, cwd) = doc_temp_dir("redirect");
        std::fs::write(dir.join("package.json"), r#"{ "name": "root" }"#).unwrap();
        std::fs::write(
            dir.join("vite.config.ts"),
            "export default {\n  defaultPackage: {\n    doc: 'packages/docs',\n  },\n};\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("packages/docs")).unwrap();
        std::fs::write(dir.join("packages/docs/package.json"), r#"{ "name": "docs" }"#).unwrap();

        // `init` and `info` follow the redirect too (`elicit` false).
        for (command, elicit) in [("doc build", true), ("doc init", false), ("doc info", false)] {
            let target = resolve_doc_target(&cwd, command, elicit).unwrap();
            let AppTarget::Dir(target) = target else {
                panic!("expected a redirect for `{command}`");
            };
            assert!(target.as_path().ends_with("packages/docs"));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn doc_redirect_ignores_the_string_form() {
        let (dir, cwd) = doc_temp_dir("string-form");
        std::fs::write(dir.join("package.json"), r#"{ "name": "root" }"#).unwrap();
        std::fs::write(
            dir.join("vite.config.ts"),
            "export default {\n  defaultPackage: './apps/web',\n};\n",
        )
        .unwrap();
        // The string form covers the app commands only, never `doc`.
        let target = resolve_doc_target(&cwd, "doc build", true).unwrap();
        assert!(matches!(target, AppTarget::CurrentDir));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn doc_redirect_reports_invalid_and_non_static_values() {
        let (dir, cwd) = doc_temp_dir("invalid");
        std::fs::write(dir.join("package.json"), r#"{ "name": "root" }"#).unwrap();
        std::fs::write(dir.join("vite.config.ts"), "export default {\n  defaultPackage: 42,\n};\n")
            .unwrap();
        // An invalid shape errors loudly instead of being ignored.
        let target = resolve_doc_target(&cwd, "doc build", true).unwrap();
        assert!(matches!(target, AppTarget::Exit(ExitStatus(1))));

        std::fs::write(
            dir.join("vite.config.ts"),
            "export default {\n  defaultPackage: pick(),\n};\n",
        )
        .unwrap();
        // A declared but non-static value fails the same way.
        let target = resolve_doc_target(&cwd, "doc build", true).unwrap();
        assert!(matches!(target, AppTarget::Exit(ExitStatus(1))));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A workspace with two documentation packages and one plain package.
    fn doc_workspace(name: &str) -> (std::path::PathBuf, AbsolutePathBuf) {
        let (dir, abs) = doc_temp_dir(name);
        std::fs::write(dir.join("package.json"), r#"{ "name": "root" }"#).unwrap();
        std::fs::write(dir.join("pnpm-workspace.yaml"), "packages:\n  - 'packages/*'\n").unwrap();
        for (member, manifest) in [
            (
                "docs",
                r#"{ "name": "docs", "devDependencies": { "@astrojs/starlight": "^0.41.0" } }"#,
            ),
            (
                "handbook",
                r#"{ "name": "handbook", "devDependencies": { "vitepress": "^2.0.0-0" } }"#,
            ),
            ("app", r#"{ "name": "app" }"#),
        ] {
            let member_dir = dir.join("packages").join(member);
            std::fs::create_dir_all(&member_dir).unwrap();
            std::fs::write(member_dir.join("package.json"), manifest).unwrap();
        }
        (dir, abs)
    }

    #[test]
    fn doc_classification_elicits_marker_members_at_a_workspace_root() {
        let (dir, cwd) = doc_workspace("elicit");
        let DocClassification::Elicit(rows) = classify_doc(&cwd).unwrap() else {
            panic!("expected candidates");
        };
        // Only marker-declaring members, sorted by path.
        assert_eq!(
            rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(),
            ["packages/docs", "packages/handbook"]
        );
        assert_eq!(rows[0].providers, "starlight");
        assert_eq!(rows[1].providers, "vitepress");

        // A member directory never elicits: detection stays local.
        let member = AbsolutePathBuf::new(dir.join("packages/docs")).unwrap();
        assert!(matches!(classify_doc(&member).unwrap(), DocClassification::RunInPlace));

        // A root that declares its own marker is its own documentation site.
        std::fs::write(
            dir.join("package.json"),
            r#"{ "name": "root", "devDependencies": { "vitepress": "^2.0.0-0" } }"#,
        )
        .unwrap();
        assert!(matches!(classify_doc(&cwd).unwrap(), DocClassification::RunInPlace));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn doc_scripts_intercept_per_invocation_shape() {
        let (dir, cwd) = doc_workspace("intercept");
        // Actions at a candidates root spawn the real binary.
        assert!(needs_elicitation(&doc_subcommand(&["build"]), &cwd));
        // `init`, `info`, and unparsable arguments spawn it anywhere.
        let member = AbsolutePathBuf::new(dir.join("packages/docs")).unwrap();
        assert!(needs_elicitation(&doc_subcommand(&["init", "vitepress"]), &member));
        assert!(needs_elicitation(&doc_subcommand(&["info"]), &member));
        assert!(needs_elicitation(&doc_subcommand(&["--bogus"]), &member));
        // An action inside the documentation package synthesizes (cached path).
        assert!(!needs_elicitation(&doc_subcommand(&["build"]), &member));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
