# Doc Config

`vp doc` reads provider selection from the `doc` block in `vite.config.ts`. Check out [`vp doc`](/guide/doc) for the command surface, the built-in providers, and dependency detection.

## Example

```ts [vite.config.ts]
import { defineConfig } from 'vite-plus';

export default defineConfig({
  doc: {
    provider: 'starlight',
  },
});
```

## `doc.provider`

- **Type:** `'vitepress' | 'vocs' | 'starlight' | 'ox-content'`
- **Default:** `undefined` (dependency detection selects the provider)

Selects the documentation provider ahead of dependency detection. Most packages omit the key: a unique provider marker in `dependencies` or `devDependencies` selects the provider by itself.

Set it when detection cannot decide. The main case is a migration, where the old and the new marker coexist until the switch completes; `doc.provider` bridges that window and leaves with the old marker. The selected provider's marker package must still be installed, so a typo or a stale value fails before the tool runs.

The `doc` block is package-level: its home is the config next to the documentation site. Execution targets always come from the built-in provider definitions; the config only picks among them.

## The `defaultPackage` `doc` entry

Naming the documentation package for a whole workspace is not a `doc` key. A workspace root names it through the `doc` entry of [`defaultPackage`](/config/#defaultpackage):

```ts [vite.config.ts]
// vite.config.ts at the workspace root
export default {
  defaultPackage: {
    doc: 'packages/docs',
  },
};
```

Every `vp doc` subcommand at that root then behaves as an implicit `-C packages/docs`, `init` and `info` included. The named package's own dependencies and `doc` block decide the provider.

Only the object form carries the entry: the string form of `defaultPackage` covers the app commands and never applies to `doc`, because the blessed app package and the documentation package are usually different packages. Like every `defaultPackage` value, the entry must stay a plain string literal so vp can read it without executing the config. A single-package project needs no entry.
