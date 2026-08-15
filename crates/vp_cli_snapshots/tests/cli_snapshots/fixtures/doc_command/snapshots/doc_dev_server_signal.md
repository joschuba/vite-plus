# doc_dev_server_signal

Ctrl+C reaches the delegated dev server; the tool decides the exit status.

## `vp doc`

**Exit code:** 130

**→ expect-milestone:** `doc-dev-server:ready`

```
Using provider `vitepress` (dependency marker `vitepress` in package.json)
fake vitepress argv: dev
fake vitepress dev server listening
```

**← write-key:** `ctrl-c`

```
Using provider `vitepress` (dependency marker `vitepress` in package.json)
fake vitepress argv: dev
fake vitepress dev server listening
fake vitepress dev: SIGINT received, shutting down
```
