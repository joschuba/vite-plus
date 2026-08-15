# Doc

`vp doc` develops, builds, and previews documentation sites through one command surface.

## Overview

Documentation tools converge on the same three operations, but each has its own executable and option syntax. `vp doc` gives them one stable command surface through a **provider**: the integration that detects your documentation tool, translates the common actions, and executes the tool.

Vite+ bundles no documentation generator and forces no single choice. The tool stays a normal project dependency, so your project controls its version, configuration, theme, and content structure. When you replace the tool, only the dependencies change; scripts, CI configuration, and hosting commands stay on `vp doc`.

The built-in providers are:

| Provider     | Tool                                                          | Detected through          | Runs                        |
| ------------ | ------------------------------------------------------------- | ------------------------- | --------------------------- |
| `vitepress`  | [VitePress 2](https://github.com/vuejs/vitepress)             | `vitepress`               | the `vitepress` bin         |
| `vocs`       | [Vocs 2](https://github.com/wevm/vocs)                        | `vocs`                    | the `vocs` bin              |
| `starlight`  | [Starlight](https://github.com/withastro/starlight)           | `@astrojs/starlight`      | the `astro` bin             |
| `ox-content` | [Ox Content](https://github.com/ubugeeei-prod/ox-content)     | `@ox-content/vite-plugin` | Vite+'s built-in Vite command |

Vite+ bundles Vite 8, so each provider supports the tool releases that run on Vite 8: VitePress `>=2.0.0-alpha.18`, Vocs `>=2.0.0`, and Starlight `>=0.41.0`. Every published Ox Content release targets Vite 8. `vp doc` reports an unsupported installed version before it runs the tool.

## Usage

```bash
vp doc                  # start the dev server (the default)
vp doc dev --host 0.0.0.0
vp doc build
vp doc preview --port 4173
vp doc init vitepress   # set up a provider
vp doc info --json      # report the resolved provider
```

Every argument after `dev`, `build`, or `preview` forwards to the tool verbatim. For the classic VitePress layout with content in `docs/`, run `vp doc dev docs`, exactly like `vitepress dev docs`.

Run tool-only subcommands such as `vocs twoslash` through `vp exec` or a package script:

```bash
vp exec vocs twoslash
```

## Provider Selection

Vite+ selects the provider in this order:

1. `doc.provider` in `vite.config.ts`
2. A unique dependency marker in the nearest `package.json`

Detection reads `dependencies` and `devDependencies`. Most projects need no configuration: declare the tool as a dependency. `vp doc` finds it.

```json [package.json]
{
  "devDependencies": {
    "vitepress": "^2.0.0-0"
  }
}
```

A package selects exactly one provider. When a package declares more than one marker, `vp doc` reports a misconfiguration and exits. The one legitimate window is a migration, where the old and the new marker coexist until the switch completes. Set `doc.provider` to bridge that window:

```ts [vite.config.ts]
import { defineConfig } from 'vite-plus';

export default defineConfig({
  doc: {
    provider: 'starlight',
  },
});
```

## Setting Up a Provider

`vp doc init [PROVIDER]` sets up a provider in place. It scaffolds the tool's starter files and never overwrites existing ones. It installs the dependencies through your package manager. When detection alone would not select the provider, it also writes `doc.provider`.

In an interactive terminal, `vp doc` with no provider offers the same setup instead of failing, and continues into the requested command after the install. In CI the command exits with status 1 and prints the exact `vp doc init` invocation to run.

## Inspecting the Selection

`vp doc info` reports the resolved provider, the tool package with its installed version, the selection source, and the supported commands. It never starts the tool. `--json` emits the same report for tooling and coding agents:

```bash
$ vp doc info --json
{
  "provider": "starlight",
  "displayName": "Starlight",
  "source": { "kind": "dependency-marker", "marker": "@astrojs/starlight" },
  "target": "package-bin",
  "tool": { "package": "astro", "version": "7.2.2", "supportedRange": ">=0.41.0", "versionSupported": true },
  "commands": ["dev", "build", "preview"]
}
```

## Monorepos

Detection stays local: `vp doc` reads the nearest `package.json` from the invocation directory. From the workspace root, name the documentation package once through the `doc` entry of [`defaultPackage`](/guide/monorepo):

```ts [vite.config.ts]
import { defineConfig } from 'vite-plus';

export default defineConfig({
  defaultPackage: {
    doc: 'packages/docs',
  },
});
```

Every `vp doc` subcommand at the root then behaves as an implicit `-C packages/docs`, including `init` and `info`. Without the entry, an interactive `vp doc` at the root opens a picker over the workspace packages that declare a provider. A non-interactive run lists each candidate as a ready-to-run `vp -C <dir> doc` command.

## Caching

Inside a task session, `vp run` recognizes a `vp doc build` script as a Vite+ build and caches it with automatic input and output tracking, like the other built-in commands. `dev` and `preview` run servers and stay uncached.

```json [package.json]
{
  "scripts": {
    "docs:build": "vp doc build"
  }
}
```
