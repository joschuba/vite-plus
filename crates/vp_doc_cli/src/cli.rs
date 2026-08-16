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

    /// `dev` and `preview` run long-lived servers; only `build` is a batch
    /// operation. Cache policy and terminal handling both key off this, so
    /// the distinction lives here once.
    pub fn is_server(self) -> bool {
        !matches!(self, DocAction::Build)
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
    Init { provider: Option<String> },
    Info { json: bool },
}

/// Parse `vp doc` arguments per the doc-command RFC. The first token
/// decides the invocation, and Vite+ consumes no option of its own: every
/// argument after `dev`, `build`, or `preview` (or after `--`) forwards to
/// the tool verbatim.
pub fn parse_doc_args(args: &[String]) -> Result<DocInvocation, Error> {
    let Some(first) = args.first().map(String::as_str) else {
        return Ok(DocInvocation::Action(DocRequest { action: DocAction::Dev, args: Vec::new() }));
    };
    let rest = args[1..].to_vec();
    if first == "--" {
        // `--` before an action ends Vite+ parsing; everything after it
        // forwards to the default `dev`.
        return Ok(DocInvocation::Action(DocRequest { action: DocAction::Dev, args: rest }));
    }
    if first.starts_with('-') {
        // A leading option selects the default `dev` and forwards the
        // complete argument sequence verbatim; `--` stays accepted as a
        // conventional separator but nothing requires it
        // (rfcs/doc-command.md, Command Interface). A lone `-h`/`--help`
        // never reaches this parse: both CLI surfaces answer it first.
        return Ok(DocInvocation::Action(DocRequest {
            action: DocAction::Dev,
            args: args.to_vec(),
        }));
    }
    let action = match first {
        "dev" => DocAction::Dev,
        "build" => DocAction::Build,
        "preview" => DocAction::Preview,
        "init" => return parse_init_args(&rest),
        "info" => return parse_info_args(&rest),
        other => {
            return Err(user_message(format!(
                "unrecognized doc command `{other}`\n\nAvailable commands: dev, build, preview, init, info"
            )));
        }
    };
    Ok(DocInvocation::Action(DocRequest { action, args: rest }))
}

fn parse_init_args(args: &[String]) -> Result<DocInvocation, Error> {
    let mut provider = None;
    for arg in args {
        if arg.starts_with('-') || provider.is_some() {
            return Err(user_message(format!(
                "unexpected argument `{arg}`\n\nUsage: vp doc init [PROVIDER]"
            )));
        }
        provider = Some(arg.clone());
    }
    Ok(DocInvocation::Init { provider })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_string()).collect()
    }

    fn action(list: &[&str]) -> DocRequest {
        match parse_doc_args(&args(list)).unwrap() {
            DocInvocation::Action(request) => request,
            other => panic!("expected an action, got {other:?}"),
        }
    }

    fn parse_error(list: &[&str]) -> String {
        match parse_doc_args(&args(list)).unwrap_err() {
            Error::UserMessage(message) => message,
            other => panic!("expected a user message, got {other:?}"),
        }
    }

    #[test]
    fn bare_doc_defaults_to_dev() {
        let request = action(&[]);
        assert_eq!(request.action, DocAction::Dev);
        assert!(request.args.is_empty());
    }

    #[test]
    fn arguments_after_the_action_forward_verbatim() {
        let request = action(&["build", "--site", "example", "--", "-x"]);
        assert_eq!(request.action, DocAction::Build);
        assert_eq!(request.args, ["--site", "example", "--", "-x"]);
    }

    #[test]
    fn double_dash_forwards_to_the_default_dev() {
        let request = action(&["--", "--port", "4173"]);
        assert_eq!(request.action, DocAction::Dev);
        assert_eq!(request.args, ["--port", "4173"]);
    }

    #[test]
    fn a_leading_option_selects_dev_and_forwards_everything() {
        let request = action(&["--host", "0.0.0.0", "--port", "3000"]);
        assert_eq!(request.action, DocAction::Dev);
        assert_eq!(request.args, ["--host", "0.0.0.0", "--port", "3000"]);
    }

    #[test]
    fn an_unknown_command_token_is_an_error() {
        let message = parse_error(&["serve"]);
        assert!(message.contains("unrecognized doc command `serve`"), "{message}");
    }

    #[test]
    fn init_takes_one_optional_provider_id() {
        let DocInvocation::Init { provider } =
            parse_doc_args(&args(&["init", "vitepress"])).unwrap()
        else {
            panic!("expected init");
        };
        assert_eq!(provider.as_deref(), Some("vitepress"));
        assert!(matches!(
            parse_doc_args(&args(&["init"])).unwrap(),
            DocInvocation::Init { provider: None }
        ));
        let message = parse_error(&["init", "vitepress", "extra"]);
        assert!(message.contains("unexpected argument `extra`"), "{message}");
        let message = parse_error(&["init", "--json"]);
        assert!(message.contains("unexpected argument `--json`"), "{message}");
    }

    #[test]
    fn info_accepts_only_json() {
        assert!(matches!(
            parse_doc_args(&args(&["info"])).unwrap(),
            DocInvocation::Info { json: false }
        ));
        assert!(matches!(
            parse_doc_args(&args(&["info", "--json"])).unwrap(),
            DocInvocation::Info { json: true }
        ));
        let message = parse_error(&["info", "--verbose"]);
        assert!(message.contains("unexpected argument `--verbose`"), "{message}");
    }
}
