---
name: sync-upstream-cli-help
description: Sync Vite+'s mirrored help labels and descriptions verbatim with Vite, Vitest, Oxlint, Oxfmt, and tsdown while preserving intentional omissions. Use after an upstream dependency upgrade produces a CLI help diff or when packages/cli/src/help.ts has drifted from the tool options Vite+ exposes.
allowed-tools: Read, Grep, Glob, Edit, Bash
---

# Sync upstream CLI help

Update the static help documents in `packages/cli/src/help.ts` from the report at
`$CLI_HELP_DIFF_REPORT`. Keep Vite+ command framing and intentional omissions, but
copy the labels and descriptions of every retained upstream help item verbatim.

## Command mapping

| Upstream help         | `commandHelpDocs` entries |
| --------------------- | ------------------------- |
| `vite --help`         | `dev`                     |
| `vite build --help`   | `build`                   |
| `vite preview --help` | `preview`                 |
| `vitest --help`       | `test`                    |
| `oxlint --help`       | `lint`                    |
| `oxfmt --help`        | `fmt`                     |
| `tsdown --help`       | `pack`                    |

## Workflow

1. Confirm `$CLI_HELP_DIFF_CHANGED` is `true`, then read `$CLI_HELP_DIFF_REPORT`.
2. Inspect only the `<summary>` sections marked `CLI help changed`. If a diff was
   truncated, rerun that exact target version with `pnpm dlx <tool>@<version> --help`;
   include Vite's `build --help` and `preview --help` where applicable.
3. Compare each changed upstream help item with the mapped entry in
   `packages/cli/src/help.ts`.
4. Update commands, arguments, options, section names, and descriptions that Vite+
   actually exposes. For each retained item, copy the upstream label and description
   verbatim, including placeholder syntax and casing, type annotations, accepted
   values, defaults, capitalization, punctuation, and multiline text. Do not
   paraphrase, shorten, or normalize upstream wording.
5. Preserve Vite+-owned command summaries, `vp` usage strings, examples, and
   documentation URLs. Do not add an intentionally omitted upstream item merely to
   make the option sets identical.
6. For removed upstream options, remove the mirrored row only after confirming Vite+
   does not deliberately retain or implement it.
7. Re-record and inspect the focused help snapshots:

   ```bash
   UPDATE_SNAPSHOTS=1 just snapshot-test command_tool_help
   git diff -- crates/vite_cli_snapshots/tests/cli_snapshots/fixtures/command_tool_help
   ```

## Intentional differences

- Ignore only presentation changes such as terminal alignment, wrapping, ANSI color,
  ordering, and version banners. Treat any wording, capitalization, punctuation,
  placeholder syntax, type annotation, accepted-value, or displayed-default change
  as actionable for an item Vite+ already shows.
- Do not expose upstream config-file selectors or loaders. Vite+ reads tool settings
  from `vite.config.ts`; known omissions include `--config`, `--configLoader`, and
  `--disable-nested-config`.
- Do not expose standalone tool-management modes that bypass the Vite+ command flow,
  such as `--init`, `--migrate`, or `--lsp`.
- Do not add upstream `--version` flags; version reporting belongs to the top-level
  `vp` command.
- Keep Vite+-owned command framing, such as `vp test` running once by default, outside
  the mirrored item descriptions. A Vite+-specific runtime behavior or default does
  not justify rewriting an upstream label or description in the mirrored help table.
- Do not change runtime argument forwarding in this skill. Use
  `.claude/skills/sync-tsdown-cli/SKILL.md` separately when tsdown's executable option
  handling also needs an update.

If a changed upstream flag falls into an intentional category or is not forwarded by
Vite+, leave the document unchanged. Do not manufacture a code change merely because
the report contains a diff.
