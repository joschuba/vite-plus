use crate::error::{Error, user_message};

/// The action of the `doc` command (rfcs/doc-command.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocAction {
    Dev,
    Build,
    Preview,
}

impl DocAction {
    pub fn as_str(self) -> &'static str {
        match self {
            DocAction::Dev => "dev",
            DocAction::Build => "build",
            DocAction::Preview => "preview",
        }
    }
}

/// An action invocation: the action and the arguments forwarded to the
/// tool verbatim.
#[derive(Debug)]
pub struct DocRequest {
    pub action: DocAction,
    pub args: Vec<String>,
}

/// A parsed `vp doc` invocation. `Init` and `Info` are Vite+-owned commands;
/// only `Action` delegates to a tool.
#[derive(Debug)]
pub enum DocInvocation {
    Action(DocRequest),
    Init { args: Vec<String> },
    Info { json: bool },
}

/// Parse `vp doc` arguments per the doc-command RFC. Vite+ consumes no
/// option of its own; every argument after `dev`, `build`, or `preview`
/// (or after `--`) forwards to the tool verbatim.
pub fn parse_doc_args(args: &[String]) -> Result<DocInvocation, Error> {
    let mut action = DocAction::Dev;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--" {
            rest.extend(args[i + 1..].iter().cloned());
            break;
        } else if arg.starts_with('-') {
            return Err(user_message(format!(
                "unexpected option `{arg}` before the doc command\n\nPlace tool options after `dev`, `build`, or `preview`, or after `--`."
            )));
        } else {
            match arg {
                "dev" => action = DocAction::Dev,
                "build" => action = DocAction::Build,
                "preview" => action = DocAction::Preview,
                "init" => {
                    return Ok(DocInvocation::Init { args: args[i + 1..].to_vec() });
                }
                "info" => {
                    return parse_info_args(&args[i + 1..]);
                }
                other => {
                    return Err(user_message(format!(
                        "unrecognized doc command `{other}`\n\nAvailable commands: dev, build, preview, init, info"
                    )));
                }
            }
            rest.extend(args[i + 1..].iter().cloned());
            break;
        }
        i += 1;
    }
    Ok(DocInvocation::Action(DocRequest { action, args: rest }))
}

fn parse_info_args(args: &[String]) -> Result<DocInvocation, Error> {
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else {
            return Err(user_message(format!(
                "unexpected argument `{arg}`\n\nUsage: vp doc info [--json]"
            )));
        }
    }
    Ok(DocInvocation::Info { json })
}
