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
# Documentation

This site runs on [VitePress](https://vitepress.dev) through `vp doc`.

## Commands

- `vp doc` starts the dev server.
- `vp doc build` builds the site for production.
- `vp doc preview` serves the production build.

## Next steps

- Add pages as Markdown files next to this one; every `.md` file becomes a route.
- Configure the site title, theme, and sidebar in `.vitepress/config.ts`; see the [VitePress guide](https://vitepress.dev/guide/what-is-vitepress).
- Read how `vp doc` selects and runs the tool in the [Vite+ doc guide](https://viteplus.dev/guide/doc).
```

## `vp doc build`

detection now selects the installed provider

```
Using provider `vitepress` (dependency marker `vitepress` in package.json)
fake vitepress argv: build
```
