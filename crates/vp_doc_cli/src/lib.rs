//! Documentation-provider infrastructure for `vp doc` (rfcs/doc-command.md).
//!
//! Follows the `vp_pm_cli` pattern: data-only provider definitions, manifest
//! detection, and command translation live here. Process execution, the
//! package-manager dispatch for `init`, and user-facing printing stay with
//! the caller.

// The same crate-level allowance as `vp_pm_cli`: errors are complete
// user-facing `String` messages, and paths interoperate with `serde_json`
// manifests.
#![allow(clippy::allow_attributes, clippy::disallowed_types, clippy::disallowed_macros)]

mod cli;
mod config;
mod detect;
mod error;
mod info;
mod init;
mod providers;
mod resolve;

pub use cli::{DocAction, DocInvocation, DocRequest, parse_doc_args};
pub use config::{
    DocConfig, DocConfigContext, StaticDocConfig, load_static_doc_config, parse_doc_config,
};
pub use detect::{
    InstalledPackage, NearestManifest, detect_providers, find_installed_package,
    find_nearest_manifest,
};
pub use error::Error;
pub use info::{DocInfoReport, DocSelectionSource, DocToolInfo, info_report};
pub use init::{
    DocConfigWrite, DocInitOutcome, ScaffoldedFile, init_scaffold, write_doc_provider_config,
};
pub use providers::{
    DOC_PROVIDERS, ProviderDefinition, ProviderInit, ProviderTarget, StarterFile, init_providers,
};
pub use resolve::{
    DocResolution, ProviderSelection, SelectionSource, no_provider_message, resolve,
    select_provider,
};
