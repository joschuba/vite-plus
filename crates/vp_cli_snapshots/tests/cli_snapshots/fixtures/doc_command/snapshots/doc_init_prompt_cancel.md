# doc_init_prompt_cancel

An interactive session offers initialization instead of failing; a declined prompt exits non-zero.

## `vp doc`

**Exit code:** 1

**→ expect-milestone:** `doc-provider-select::0`

```
No documentation provider is configured.
Select a documentation provider (↑/↓, Enter to confirm):

  › VitePress  Vue · recommended
    Starlight  Astro
    Ox Content framework-agnostic
```

**← write-key:** `down`

**→ expect-milestone:** `doc-provider-select::1`

```
No documentation provider is configured.
Select a documentation provider (↑/↓, Enter to confirm):

    VitePress  Vue · recommended
  › Starlight  Astro
    Ox Content framework-agnostic
```

**← write-key:** `ctrl-c`

```
No documentation provider is configured.
```
