//! Data-only definitions of the built-in documentation providers.
//!
//! The RFC's built-in providers (rfcs/doc-command.md): VitePress 2, Vocs,
//! Starlight, and Ox Content, with their pinned Vite 8 floors. Detection
//! and resolution stay generic; new providers join by adding definitions
//! here.

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

/// A provider-specific native-config validation that runs before
/// execution: a dependency marker cannot prove that an integration is
/// active (rfcs/doc-command.md, Built-in Providers).
#[derive(Debug, Clone, Copy)]
pub enum NativeConfigCheck {
    /// The effective root's Vite config file must mention this package.
    ViteConfigMentions(&'static str),
    /// One of these files must exist in the effective root.
    FileExists(&'static [&'static str]),
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
    /// The known-incompatible boundary: a version below this floor fails
    /// hard, while a version above `version_range` only warns
    /// (rfcs/doc-command.md, Unsupported tool version).
    pub version_floor: Option<&'static str>,
    /// Native-config validation before execution; `None` for standalone
    /// CLI tools whose marker is the executable.
    pub native_config: Option<NativeConfigCheck>,
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
        display_name: "VitePress",
        marker: "vitepress",
        marker_hint: Some("major version 2"),
        // `2.0.0-alpha.18` is the first VitePress release on Vite 8; the
        // earlier 2.0 alphas run Vite 7 or older.
        version_range: Some(">=2.0.0-alpha.18 <3.0.0"),
        version_floor: Some("2.0.0-alpha.18"),
        native_config: None,
        vite_requirement: ">=8.0.0",
        capabilities: &[DocAction::Dev, DocAction::Build, DocAction::Preview],
        target: ProviderTarget::PackageBin { package_name: "vitepress", bin_name: "vitepress" },
        init: Some(ProviderInit {
            // A range spec, never a mutable dist-tag: `vitepress@next`
            // would install VitePress 3 the moment the tag moves, and the
            // same binary would then reject it (rfcs/doc-command.md,
            // Initialization). The `vite:doc` create template carries its
            // own VitePress starter
            // (packages/cli/src/create/templates/doc.ts); keep the two in
            // step when the VitePress line or the first page changes.
            dependencies: &["vitepress@>=2.0.0-alpha.18 <3.0.0"],
            starter_files: &[StarterFile {
                path: "index.md",
                content: "# Documentation\n\nThis site runs on [VitePress](https://vitepress.dev) through `vp doc`.\n\n## Commands\n\n- `vp doc` starts the dev server.\n- `vp doc build` builds the site for production.\n- `vp doc preview` serves the production build.\n\n## Next steps\n\n- Add pages as Markdown files next to this one; every `.md` file becomes a route.\n- Configure the site title, theme, and sidebar in `.vitepress/config.ts`; see the [VitePress guide](https://vitepress.dev/guide/what-is-vitepress).\n- Read how `vp doc` selects and runs the tool in the [Vite+ doc guide](https://viteplus.dev/guide/doc).\n",
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
        marker_hint: None,
        version_range: Some(">=2.0.0"),
        version_floor: Some("2.0.0"),
        native_config: None,
        vite_requirement: ">=8.0.0",
        capabilities: &[DocAction::Dev, DocAction::Build, DocAction::Preview],
        target: ProviderTarget::PackageBin { package_name: "vocs", bin_name: "vocs" },
        init: None,
    },
    ProviderDefinition {
        // The marker and the executable differ: Starlight is detected
        // through `@astrojs/starlight` and executed through `astro`.
        // `0.41.0` is the first release whose Astro peer admits only
        // Astro 7, the first Astro major on Vite 8.
        id: "starlight",
        display_name: "Starlight",
        marker: "@astrojs/starlight",
        marker_hint: None,
        version_range: Some(">=0.41.0"),
        version_floor: Some("0.41.0"),
        native_config: Some(NativeConfigCheck::FileExists(&[
            "astro.config.mjs",
            "astro.config.mts",
            "astro.config.js",
            "astro.config.ts",
        ])),
        vite_requirement: ">=8.0.0",
        capabilities: &[DocAction::Dev, DocAction::Build, DocAction::Preview],
        target: ProviderTarget::PackageBin { package_name: "astro", bin_name: "astro" },
        init: Some(ProviderInit {
            dependencies: &["astro", "@astrojs/starlight"],
            starter_files: &[
                StarterFile {
                    path: "astro.config.mjs",
                    content: "import { defineConfig } from 'astro/config';\nimport starlight from '@astrojs/starlight';\n\nexport default defineConfig({\n  integrations: [starlight({ title: 'Docs' })],\n});\n",
                },
                StarterFile {
                    path: "src/content.config.ts",
                    content: "import { defineCollection } from 'astro:content';\nimport { docsLoader } from '@astrojs/starlight/loaders';\nimport { docsSchema } from '@astrojs/starlight/schema';\n\nexport const collections = {\n  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),\n};\n",
                },
                StarterFile {
                    path: "src/content/docs/index.md",
                    content: "---\ntitle: Documentation\n---\n\nThis site runs on [Starlight](https://starlight.astro.build) through `vp doc`.\n\n## Commands\n\n- `vp doc` starts the dev server.\n- `vp doc build` builds the site for production.\n- `vp doc preview` serves the production build.\n\n## Next steps\n\n- Add pages as `.md` or `.mdx` files under `src/content/docs/`.\n- Configure the site title and sidebar in `astro.config.mjs`; see the [Starlight guide](https://starlight.astro.build/getting-started/).\n- Read how `vp doc` selects and runs the tool in the [Vite+ doc guide](https://viteplus.dev/guide/doc).\n",
                },
            ],
            prompt_hint: "Astro",
        }),
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
        version_floor: None,
        // The marker cannot prove the plugin is registered; without it the
        // built-in target would build the application instead of the
        // documentation (rfcs/doc-command.md, Built-in Providers).
        native_config: Some(NativeConfigCheck::ViteConfigMentions("@ox-content/vite-plugin")),
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
                    content: "# Documentation\n\nThis site runs on [Ox Content](https://github.com/ubugeeei-prod/ox-content) through `vp doc`.\n\n## Commands\n\n- `vp doc` starts the dev server.\n- `vp doc build` builds the site for production.\n- `vp doc preview` serves the production build.\n\n## Next steps\n\n- Add pages as Markdown files under `docs/`.\n- Configure the plugin (for example `srcDir`) in `vite.config.ts`; see the [Ox Content readme](https://github.com/ubugeeei-prod/ox-content).\n- Read how `vp doc` selects and runs the tool in the [Vite+ doc guide](https://viteplus.dev/guide/doc).\n",
                },
                StarterFile {
                    path: "index.html",
                    content: "<!doctype html>\n<html>\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>Docs</title>\n  </head>\n  <body>\n    <div id=\"app\"></div>\n  </body>\n</html>\n",
                },
            ],
            prompt_hint: "framework-agnostic",
        }),
    },
];

/// Providers that declare init support, in prompt order (VitePress first).
pub fn init_providers() -> impl Iterator<Item = &'static ProviderDefinition> {
    DOC_PROVIDERS.iter().filter(|provider| provider.init.is_some())
}
