# builtin_framework_guard

## `vp dev`

`vp dev` refuses in a Nuxt project and points at the dev script

**Exit code:** 1

```
VITE+ - The Unified Toolchain for the Web

error: this project uses Nuxt (nuxt.config.ts). `vp dev` runs the bundled Vite CLI, not the Nuxt CLI.
hint: did you mean `vp run dev`?
```

## `vp build`

`vp build` refuses the same way

**Exit code:** 1

```
VITE+ - The Unified Toolchain for the Web

error: this project uses Nuxt (nuxt.config.ts). `vp build` runs the bundled Vite CLI, not the Nuxt CLI.
hint: did you mean `vp run build`?
```

## `cd src && vp dev`

the refusal finds the enclosing package from a subdirectory, with the same walk `vp run` uses

**Exit code:** 1

```
VITE+ - The Unified Toolchain for the Web

error: this project uses Nuxt (nuxt.config.ts). `vp dev` runs the bundled Vite CLI, not the Nuxt CLI.
hint: did you mean `vp run dev`?
```

## `cd astro && vp dev`

an Astro config triggers the same refusal

**Exit code:** 1

```
VITE+ - The Unified Toolchain for the Web

error: this project uses Astro (astro.config.mjs). `vp dev` runs the bundled Vite CLI, not the Astro CLI.
hint: did you mean `vp run dev`?
```

## `cd no-scripts && vp dev`

without scripts, the hint points at the framework CLI through `vp exec`

**Exit code:** 1

```
VITE+ - The Unified Toolchain for the Web

error: this project uses Nuxt (nuxt.config.ts). `vp dev` runs the bundled Vite CLI, not the Nuxt CLI.
hint: run the Nuxt CLI with `vp exec nuxt dev`.
```

## `cd no-scripts && vp build`

`vp build` gets the same fallback hint

**Exit code:** 1

```
VITE+ - The Unified Toolchain for the Web

error: this project uses Nuxt (nuxt.config.ts). `vp build` runs the bundled Vite CLI, not the Nuxt CLI.
hint: run the Nuxt CLI with `vp exec nuxt build`.
```

## `cd renamed-script && vp dev`

a script that runs the framework dev command under another name becomes the hint target

**Exit code:** 1

```
VITE+ - The Unified Toolchain for the Web

error: this project uses Nuxt (nuxt.config.ts). `vp dev` runs the bundled Vite CLI, not the Nuxt CLI.
hint: did you mean `vp run start`? The start script runs `nuxi dev`.
```

## `cd renamed-script && vp build`

the build hint finds the renamed build script the same way

**Exit code:** 1

```
VITE+ - The Unified Toolchain for the Web

error: this project uses Nuxt (nuxt.config.ts). `vp build` runs the bundled Vite CLI, not the Nuxt CLI.
hint: did you mean `vp run make`? The make script runs `nuxt build`.
```

## `vp dev --config vite.config.ts --port 12312312312`

an explicit --config selects the bundled Vite CLI on purpose, so only the script note prints (the invalid port stops the server immediately)

**Exit code:** 1

```
VITE+ - The Unified Toolchain for the Web

note: You are running `vp dev` as a Vite+ built-in command. If you meant to run the dev npm script, use `vpr dev` instead.
error when starting dev server:
Error: No available ports found between 12312312312 and 65535
```

## `vp run dev`

`vp run dev` runs the dev script that the refusal points at

```
VITE+ - The Unified Toolchain for the Web

$ vpt print nuxt dev script ⊘ cache disabled
nuxt dev script
```
