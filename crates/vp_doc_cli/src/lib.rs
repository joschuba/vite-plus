//! Documentation-provider infrastructure for `vp doc` (rfcs/doc-command.md).
//!
//! Follows the `vp_pm_cli` pattern: a data-only provider registry, manifest
//! detection, and command translation live here. Process execution, the
//! package-manager dispatch for `init`, and user-facing printing stay with
//! the caller.

mod cli;
mod detect;
mod error;
mod info;
mod init;
mod providers;
mod resolve;

pub use cli::{DocAction, DocInvocation, DocRequest, parse_doc_args};
pub use detect::{
    InstalledPackage, NearestManifest, detect_providers, find_installed_package,
    find_nearest_manifest,
};
pub use error::Error;
pub use info::{DocInfoReport, DocSelectionSource, DocToolInfo, info_report};
pub use init::{DocInitOutcome, ScaffoldedFile, init_scaffold};
pub use providers::{
    DOC_PROVIDERS, ProviderDefinition, ProviderInit, ProviderTarget, StarterFile,
};
pub use resolve::{DocResolution, resolve};
