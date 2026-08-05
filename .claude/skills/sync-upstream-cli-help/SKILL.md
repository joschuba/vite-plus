---
name: sync-upstream-cli-help
description: Sync Vite+'s mirrored help documents with semantic CLI changes from Vite, Vitest, Oxlint, Oxfmt, and tsdown. Use after an upstream dependency upgrade produces a CLI help diff or when packages/cli/src/help.ts has drifted from the tool options Vite+ exposes.
allowed-tools: Read, Grep, Glob, Edit, Bash
---

# Sync upstream CLI help

Update the static help documents in `packages/cli/src/help.ts` from the report at
`$CLI_HELP_DIFF_REPORT`. Keep Vite+ terminology and intentional omissions; do not
blindly copy upstream output.

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
3. Compare semantic changes with the mapped entry in `packages/cli/src/help.ts`.
4. Update commands, arguments, options, section names, and descriptions that Vite+
   actually exposes. Preserve the existing `vp` usage strings, examples,
   documentation URLs, capitalization, and concise description style.
5. For removed upstream options, remove the mirrored row only after confirming Vite+
   does not deliberately retain or implement it.
6. Re-record and inspect the focused help snapshots:

   ```bash
   UPDATE_SNAPSHOTS=1 just snapshot-test command_tool_help
   git diff -- crates/vite_cli_snapshots/tests/cli_snapshots/fixtures/command_tool_help
   ```

## Intentional differences

- Ignore formatting-only changes: whitespace, wrapping, alignment, color, ordering,
  and version banners. Wording changes matter only when they change meaning, accepted
  values, or defaults.
- Do not expose upstream config-file selectors or loaders. Vite+ reads tool settings
  from `vite.config.ts`; known omissions include `--config`, `--configLoader`, and
  `--disable-nested-config`.
- Do not expose standalone tool-management modes that bypass the Vite+ command flow,
  such as `--init`, `--migrate`, or `--lsp`.
- Do not add upstream `--version` flags; version reporting belongs to the top-level
  `vp` command.
- Keep Vite+-specific behavior, including `vp test` running once by default and
  `vp pack --env-prefix` defaulting to `VITE_PACK_,TSDOWN_`.
- Do not change runtime argument forwarding in this skill. Use
  `.claude/skills/sync-tsdown-cli/SKILL.md` separately when tsdown's executable option
  handling also needs an update.

If a changed upstream flag falls into an intentional category or is not forwarded by
Vite+, leave the document unchanged. Do not manufacture a code change merely because
the report contains a diff.
