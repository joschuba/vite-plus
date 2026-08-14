//! Data-only registry of documentation providers.
//!
//! PoC scope (rfcs/doc-command.md): VitePress 2 as a package-bin target and
//! Ox Content as the built-in Vite target, plus a VuePress entry that
//! exercises the capability gate. The remaining RFC providers join by adding
//! entries here; detection and resolution stay generic.

use crate::cli::DocAction;

/// How a provider executes its tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTarget {
    PackageBin { package_name: &'static str, bin_name: &'static str },
    BuiltinVite,
}

impl ProviderTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderTarget::PackageBin { .. } => "package-bin",
            ProviderTarget::BuiltinVite => "builtin-vite",
        }
    }
}

/// A file written by `vp doc init` only when missing.
#[derive(Debug)]
pub struct StarterFile {
    pub path: &'static str,
    pub content: &'static str,
}

/// One-command setup support for `vp doc init`.
#[derive(Debug)]
pub struct ProviderInit {
    /// Dependency specs added through the project's package manager.
    pub dependencies: &'static [&'static str],
    /// Files written only when missing, relative to the effective root.
    pub starter_files: &'static [StarterFile],
    /// Shown next to the provider in the init select prompt: the UI
    /// framework pages can embed (rfcs/doc-command.md, Initialization).
    pub prompt_hint: &'static str,
}

#[derive(Debug)]
pub struct ProviderDefinition {
    pub id: &'static str,
    /// Human name used in diagnostics.
    pub display_name: &'static str,
    /// Declared dependency that selects this provider.
    pub marker: &'static str,
    /// Extra hint rendered next to the marker in the no-provider error.
    pub marker_hint: Option<&'static str>,
    /// Supported npm semver range for the marker package.
    pub version_range: Option<&'static str>,
    /// Lifecycle commands this provider supports. `build` is always required.
    pub capabilities: &'static [DocAction],
    pub target: ProviderTarget,
    /// One-command setup support for `vp doc init`.
    pub init: Option<ProviderInit>,
}

pub static DOC_PROVIDERS: &[ProviderDefinition] = &[
    ProviderDefinition {
        id: "vitepress",
        display_name: "VitePress 2",
        marker: "vitepress",
        marker_hint: Some("major version 2"),
        version_range: Some(">=2.0.0-0 <3.0.0"),
        capabilities: &[DocAction::Dev, DocAction::Build, DocAction::Preview],
        target: ProviderTarget::PackageBin { package_name: "vitepress", bin_name: "vitepress" },
        init: Some(ProviderInit {
            // `next` is the VitePress 2 dist-tag while 2.0 is prerelease; a
            // range spec replaces it when 2.0 reaches `latest`.
            dependencies: &["vitepress@next"],
            starter_files: &[StarterFile {
                path: "index.md",
                content: "# Hello VitePress\n\nStart the dev server with `vp doc`.\n",
            }],
            prompt_hint: "Vue",
        }),
    },
    ProviderDefinition {
        id: "ox-content",
        display_name: "Ox Content",
        marker: "@ox-content/vite-plugin",
        marker_hint: None,
        version_range: None,
        capabilities: &[DocAction::Dev, DocAction::Build, DocAction::Preview],
        target: ProviderTarget::BuiltinVite,
        init: Some(ProviderInit {
            dependencies: &["@ox-content/vite-plugin"],
            starter_files: &[
                StarterFile {
                    path: "vite.config.ts",
                    content: "import { oxContent } from '@ox-content/vite-plugin';\n\nexport default {\n  plugins: [oxContent({ srcDir: 'docs' })],\n};\n",
                },
                StarterFile {
                    path: "docs/index.md",
                    content: "# Hello Ox Content\n\nStart the dev server with `vp doc`.\n",
                },
                StarterFile {
                    path: "index.html",
                    content: "<!doctype html>\n<html>\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>Docs</title>\n  </head>\n  <body>\n    <div id=\"app\"></div>\n  </body>\n</html>\n",
                },
            ],
            prompt_hint: "Vite plugin",
        }),
    },
    ProviderDefinition {
        // VuePress has no native preview command. The RFC defers this
        // provider; the PoC includes it to exercise the capability gate with
        // the RFC's own example. No init metadata, so it stays out of the
        // init prompt and hint list.
        id: "vuepress",
        display_name: "VuePress 2",
        marker: "vuepress",
        marker_hint: Some("major version 2"),
        version_range: Some(">=2.0.0-0 <3.0.0"),
        capabilities: &[DocAction::Dev, DocAction::Build],
        target: ProviderTarget::PackageBin { package_name: "vuepress", bin_name: "vuepress" },
        init: None,
    },
];

/// Providers that declare init support, in prompt order (VitePress first).
pub fn init_providers() -> impl Iterator<Item = &'static ProviderDefinition> {
    DOC_PROVIDERS.iter().filter(|provider| provider.init.is_some())
}
