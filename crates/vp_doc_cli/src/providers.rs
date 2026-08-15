//! Data-only definitions of the built-in documentation providers.
//!
//! PoC scope (rfcs/doc-command.md): the RFC's built-in providers
//! (VitePress 2, Vocs, Starlight, Ox Content) with their pinned Vite 8
//! floors, plus a VuePress entry that exercises the capability gate.
//! Detection and resolution stay generic; new providers join by adding
//! definitions here.

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
    /// Supported npm semver range for the marker package. The floor is the
    /// tool's first release on Vite 8 (rfcs/doc-command.md, The Vite
    /// requirement); `None` means every published release satisfies it.
    pub version_range: Option<&'static str>,
    /// The Vite range the tool's supported releases run on. Declarative
    /// data; enforcement rides `version_range`.
    pub vite_requirement: &'static str,
    /// The actions this provider supports. `build` is always required.
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
        // `2.0.0-alpha.18` is the first VitePress release on Vite 8; the
        // earlier 2.0 alphas run Vite 7 or older.
        version_range: Some(">=2.0.0-alpha.18 <3.0.0"),
        vite_requirement: ">=8.0.0",
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
        // Vocs 1 pins Vite 7; Vocs 2 is the first line that declares a
        // `vite: ^8` peer. Init support is not planned (rfcs/doc-command.md).
        id: "vocs",
        display_name: "Vocs",
        marker: "vocs",
        marker_hint: Some("major version 2"),
        version_range: Some(">=2.0.0"),
        vite_requirement: ">=8.0.0",
        capabilities: &[DocAction::Dev, DocAction::Build, DocAction::Preview],
        target: ProviderTarget::PackageBin { package_name: "vocs", bin_name: "vocs" },
        init: None,
    },
    ProviderDefinition {
        // The marker and the executable differ: Starlight is detected
        // through `@astrojs/starlight` and executed through `astro`.
        // `0.41.0` is the first release whose Astro peer admits only
        // Astro 7, the first Astro major on Vite 8. Init support is a PoC
        // follow-up; the RFC ships it in version 1.
        id: "starlight",
        display_name: "Starlight",
        marker: "@astrojs/starlight",
        marker_hint: None,
        version_range: Some(">=0.41.0"),
        vite_requirement: ">=8.0.0",
        capabilities: &[DocAction::Dev, DocAction::Build, DocAction::Preview],
        target: ProviderTarget::PackageBin { package_name: "astro", bin_name: "astro" },
        init: None,
    },
    ProviderDefinition {
        // Every published release targets Vite 8, and from 1.1.0 the
        // plugin's `vite` peer aliases the Vite+ core package, so no
        // version floor is needed.
        id: "ox-content",
        display_name: "Ox Content",
        marker: "@ox-content/vite-plugin",
        marker_hint: None,
        version_range: None,
        vite_requirement: ">=8.0.0",
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
        // `2.0.0-rc.27` is the first RC whose Vite bundler declares Vite 8.
        version_range: Some(">=2.0.0-rc.27 <3.0.0"),
        vite_requirement: ">=8.0.0",
        capabilities: &[DocAction::Dev, DocAction::Build],
        target: ProviderTarget::PackageBin { package_name: "vuepress", bin_name: "vuepress" },
        init: None,
    },
];

/// Providers that declare init support, in prompt order (VitePress first).
pub fn init_providers() -> impl Iterator<Item = &'static ProviderDefinition> {
    DOC_PROVIDERS.iter().filter(|provider| provider.init.is_some())
}
