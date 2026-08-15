# doc_no_provider

A non-interactive session exits 1 and prints the one-command fix.

## `vp doc build`

**Exit code:** 1

```
error: no documentation provider is configured

Run `vp doc init vitepress` to set up VitePress (recommended), or
`vp doc init starlight`, `vp doc init ox-content`.

Or add one of these project dependencies yourself:
  vitepress (major version 2)
  vocs
  @astrojs/starlight
  @ox-content/vite-plugin

In a workspace, set `defaultPackage.doc` or run `vp -C <dir> doc` from the
documentation package.
```
