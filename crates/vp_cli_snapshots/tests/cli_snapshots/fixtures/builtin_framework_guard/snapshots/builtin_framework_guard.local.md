# builtin_framework_guard

## `vp dev`

`vp dev` refuses in a Nuxt project and points at the dev script

**Exit code:** 1

```
error: this project uses Nuxt (nuxt.config.ts), but `vp dev` runs the bundled Vite CLI, not the Nuxt CLI.
hint: did you mean `vp run dev`?
```

## `vp build`

`vp build` refuses the same way

**Exit code:** 1

```
error: this project uses Nuxt (nuxt.config.ts), but `vp build` runs the bundled Vite CLI, not the Nuxt CLI.
hint: did you mean `vp run build`?
```

## `cd src && vp dev`

the refusal reaches the enclosing package from a subdirectory, like `vp run` does

**Exit code:** 1

```
error: this project uses Nuxt (nuxt.config.ts), but `vp dev` runs the bundled Vite CLI, not the Nuxt CLI.
hint: did you mean `vp run dev`?
```

## `cd astro && vp dev`

an Astro config triggers the same refusal

**Exit code:** 1

```
error: this project uses Astro (astro.config.mjs), but `vp dev` runs the bundled Vite CLI, not the Astro CLI.
hint: did you mean `vp run dev`?
```

## `vp dev --config vite.config.ts --port 12312312312`

an explicit --config selects the bundled Vite CLI on purpose, so only the script note prints (invalid port exits the server immediately)

**Exit code:** 1

```
note: You are running `vp dev` as a Vite+ built-in command. If you meant to run the dev npm script, use `vpr dev` instead.
error when starting dev server:
Error: No available ports found between 12312312312 and 65535
```

## `vp run dev`

`vp run dev` runs the dev script the refusal points at

```
$ vpt print nuxt dev script ⊘ cache disabled
nuxt dev script
```
