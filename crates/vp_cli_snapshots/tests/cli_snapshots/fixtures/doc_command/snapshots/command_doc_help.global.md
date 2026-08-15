# command_doc_help

`vp doc --help` renders Vite+ help without loading a tool.

## `vp doc --help`

```
VITE+ - The Unified Toolchain for the Web

Usage: vp doc [OPTIONS] [COMMAND] [TOOL_ARGS]...

Run the project's documentation tool through its provider.
Arguments after the command are forwarded to the tool.

Commands:
  dev      Start the documentation development server [default]
  build    Build documentation for production
  preview  Preview the production build
  init     Set up a documentation provider
  info     Report the resolved provider and its capabilities

Options:
  -h, --help  Print help

Examples:
  vp doc
  vp doc dev --host 0.0.0.0
  vp doc build
  vp doc init vitepress
  vp doc info --json
  vp -C packages/docs doc build

Documentation: https://viteplus.dev/guide/doc
```
