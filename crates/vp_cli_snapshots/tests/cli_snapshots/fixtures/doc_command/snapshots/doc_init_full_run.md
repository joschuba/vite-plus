# doc_init_full_run

A full `vp doc init vitepress` run: scaffold, install the fake `vitepress@next` from the mock registry, then the action runs the installed tool.

## `vp doc init vitepress`

scaffold and install from the mock registry

```
Created index.md.
Installing vitepress@>=2.0.0-alpha.18 <3.0.0...

devDependencies:
 vitepress 2.0.0-alpha.19

Done in <duration> using pnpm <version>
VitePress is ready. Run `vp doc` to start the dev server.
```

## `vpt print-file index.md`

the scaffolded first page

```
# Hello VitePress

Start the dev server with `vp doc`.
```

## `vp doc build`

detection now selects the installed provider

```
Using provider `vitepress` (dependency marker `vitepress` in package.json)
fake vitepress argv: build
```
