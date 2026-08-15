# doc_multiple_providers

More than one marker in a manifest is a misconfiguration.

## `vp doc build`

**Exit code:** 1

```
error: multiple documentation providers are declared: vitepress, vocs

Remove the markers you do not use, or set `doc.provider` in vite.config.ts
during a migration.
```
