# doc_workspace_picker

Interactive at a workspace root: the picker lists only marker-declaring packages, then the selection runs.

## `vp doc`

**→ expect-milestone:** `doc-package-select::0`

```
Select a documentation package (↑/↓, Enter to run, type to search):

  › docs     packages/docs
    handbook packages/handbook
```

**← write-key:** `enter`

```
Selected package: docs (packages/docs)
Tip: run this directly with `vp -C packages/docs doc`
Using provider `starlight` (dependency marker `@astrojs/starlight` in package.json)
fake astro argv: dev
```
