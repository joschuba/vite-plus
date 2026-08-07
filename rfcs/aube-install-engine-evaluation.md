# Internal report: aube as the `vp install` engine

Status: internal, pre-decision. Written 2026-08-06; POC appendix added 2026-08-07.
Scope: evaluate the speed, feature, and stability claims of [aube](https://github.com/jdx/aube) and [nub](https://nubjs.com/). Compare them with pnpm 11 and with the Rust pnpm (pnpm v12). Assess the risk of a default switch for `vp install`.

## Verdict

The aube speed claims reproduce on our hardware. The speed comes from the global virtual store, a store-layout default, not from Rust. With the same store layout on both sides, the gap nearly closes: pnpm 11 with its experimental global virtual store lands within 1.2x of aube through vp, and the pnpm 12 RC within 1.5x. Feature compatibility with pnpm is strong at the lockfile layer and broken at the policy and environment layers. One bug broke vite-plus itself under aube's default store. Fixes on both sides mitigate it now (section 4). Ship aube only as an opt-in engine for now. The default switch needs the gate list at the end of this report.

## 1. What the projects are

| | aube | nub | pnpm v12 (Rust pnpm) |
| --- | --- | --- | --- |
| Repo | jdx/aube | nubjs/nub | pnpm/pnpm |
| First commit | 2026-04-18 | 2026-06-03 | rewrite merged May 2026 (pacquet archived 2026-07-21) |
| Version | 1.37.0 (2026-07-31) | ~0.6.x | 12.0.0-rc.0 (2026-08-05) |
| Language | Rust (~178k lines, 15 crates) | Rust, embeds aube | Rust engine + TS commands |
| License | MIT | MIT | MIT |
| Backing | en.dev (jdx, the mise author) | same circle | pnpm team (Zoltan Kochan) |
| Issue tracker | disabled; a bot closes PRs daily | open (37 issues) | open |

aube is the engine: resolver, linker, store, lockfile codecs, script runner. nub is one embedder. nub wires aube behind a CLI that accepts pnpm's flags. The proposed vp model copies the nub model, so most nub findings apply to vp directly.

pnpm v12 is the pnpm team's own Rust rewrite. Phase 1 moves fetch and link to Rust; TypeScript still owns resolution and the lockfile write. The RC shipped on 2026-08-05. Kochan states v12 keeps v11 behavior.

## 2. Speed

### Published claims

- aube: warm install 7x faster than pnpm, repeat test commands up to 48x faster (aube 1.37 vs pnpm 11.18, hermetic registry).
- nub: warm frozen install 10x faster than pnpm (346 ms vs 3,453 ms, 1,168 packages).
- pnpm v12: warm install 381 ms down to 12 ms vs v11, clean install 6.5 s down to 2.2 s (official, July 2026).

### Our measurements

Environment:

- macOS arm64, Node 24.19.0, timed with hyperfine.
- The cold scenario ran against registry.npmjs.org. Warm and repeat scenarios run from local stores and do not touch the registry.
- Fixture: aube's own benchmark manifest, ~1302 packages after resolution.
- One shared `pnpm-lock.yaml` (v9) for every tool.
- Per-tool isolation for HOME, store, and cache.

This mirrors the aube methodology with one deliberate change: aube runs on the pnpm lockfile. That is the vp impersonation scenario. GVS = global virtual store.

The primary comparison is the vp POC matrix of 2026-08-07: eight setups, one hyperfine pass, vp v0.2.8 with the embed hook (appendix A). The pnpm setups run through vp's managed package-manager path. nub runs standalone because vp rejects `devEngines.packageManager: nub`. The GVS-off setups disable the global virtual store per project. One config note: the embedded aube honors `enableGlobalVirtualStore: false` in `pnpm-workspace.yaml`, but nub ignores that key there and needs `enable-global-virtual-store=false` in `.npmrc`. The nub GVS-off setup sets both, and the matrix verifies store placement before the timed runs.

The two scenarios:

- Warm install: the package store and cache are already full; `node_modules` is deleted before each run. This is "clone or branch-switch on a machine that installed before".
- Repeat install: everything is installed and current; the command finds no work to do. This is "run `install` again to be safe", the most frequent case in a developer loop.

| Setup | Warm install | Repeat install |
| --- | --- | --- |
| aube 1.37 CLI (standalone, GVS on) | **361 ms** | 290 ms |
| vp + embedded aube 1.37 (GVS on) | 374 ms | 289 ms |
| vp + managed pnpm 11.20 + GVS | 439 ms | 193 ms |
| nub 0.7.2 CLI (GVS on) | 528 ms | 132 ms |
| vp + managed pnpm 12 RC + GVS | 579 ms | **20 ms** |
| nub 0.7.2 CLI (GVS off) | 1.65 s | 24 ms |
| vp + managed pnpm 11.20 | 2.66 s | 204 ms |
| vp + embedded aube 1.37 (GVS off) | 3.02 s | 268 ms |
| vp + managed pnpm 12 RC (default) | 11.2 s | **20 ms** |

Stability: we repeated the six shared setups in four full passes and the eight-setup matrix in two full passes, all on the same hardware. Warm-install means vary by 2.3-3.5% per setup between passes; repeat-install means vary by 2-7.4%. The ranking never changed. One early pass produced a high outlier for vp + pnpm 11 warm (3.41 s); the stable value is the 2.7 s band shown here. The two widest spreads: the nub GVS-on repeat mean carries one slow first run (111-132 ms across passes), and the nub GVS-off warm mean varied 1.65-1.95 s across passes.

Cold install (standalone CLIs; store, cache, and `node_modules` wiped; live network against registry.npmjs.org, 3 runs): pnpm 11 at 43.3 s, aube at 44.8 s, pnpm 12 RC at 49.1 s. The three tools land within 15% of each other, and the ordering flips between registries (an earlier npmmirror pass measured pnpm 11 at 34.7 s, pnpm 12 at 45.0 s, aube at 50.9 s). Network transfer dominates the cold scenario. The store layout that wins warm installs does not help the first download. aube's own hermetic table shows the same shape: cold aube (6.71 s) ties pnpm (6.65 s), and bun (1.45 s) leads.

### What the numbers mean

- The 7x warm claim reproduces: 7.13x engine-to-engine in the standalone matrix, and 7.1x through vp (2.66 s vs 374 ms). The claim is accurate. Their docs state the caveat themselves.
- The cause is the store model, not the language. aube links `node_modules` out of a shared virtual store that already holds the files. With GVS off, vp + aube (3.02 s) trails vp + pnpm 11 (2.66 s). The store proves it from the other side too: pnpm 11 with `enableGlobalVirtualStore: true` (experimental, available today) lands at 439 ms through vp, within 1.2x of aube, with the JS bootstrap as the remaining gap. pnpm 12's GVS row (579 ms) is currently slower than pnpm 11's because the RC materializer is unfinished (see below).
- CI installs do not get the headline number. aube forces per-project mode when `CI=1`. The CI-relevant comparison is the GVS-off row, where aube holds no lead over pnpm 11.
- The GVS default also turns itself off for projects that depend on `next`, `nuxt`, or `parcel`. Their module resolvers follow the store realpath out of the project and then cannot find project files. Those projects get the GVS-off row by design.
- aube's published 7 ms "already installed" number belongs to its `run`-path short circuit, not to `aube install`. Repeat installs are aube's weak scenario: 268-290 ms in every aube setup, against 204 ms for vp + pnpm 11 and 20 ms for vp + pnpm 12.
- The pnpm 12 RC default (non-GVS) warm result (11.2 s through vp) is an RC regression on macOS, not a design property. See "Why the pnpm 12 RC default is slow" below. Retest it at stable.
- CLI startup stays out of this comparison. vp dispatch wraps every engine, and the repeat-install row already captures the end-to-end floor per tool.
- nub runs the same engine as aube but wins every repeat-install comparison against it: 132 ms with GVS on, 24 ms with GVS off. nub puts a staleness check in front of the engine and skips the store revalidation when the state file is fresh. Without the global store to revalidate, that check is nearly as fast as pnpm 12's native short circuit. vp can copy the check for either engine.
- nub's warm GVS-off result (1.65 s) beats aube's own GVS-off path (3.02 s) by 1.8x. nub vendors a newer engine build with a different per-project linker; the gap deserves a look before vp copies either.

### Why the pnpm 12 RC default is slow

pnpm 12 should not lose to pnpm 11, and at the mechanism level it does not: the slow row is one unfinished code path, the default (non-GVS) materialization of `node_modules` on macOS. We probed it on the same fixture (2026-08-08):

- The timing signature: the same warm install costs pnpm 11 about 8-11 s of kernel time; the pnpm 12 RC costs 141-152 s of kernel time across ~13 threads, inside 11 s of wall clock.
- The disk signature: one warm install adds 81 MB of disk blocks under pnpm 11 but 296 MB under pnpm 12. Both produce per-file inodes (no hardlinks into the store), so pnpm 11's clone path shares blocks through APFS while the RC's Rust materializer writes about 4x the physical data for the same tree.

Three facts say this is temporary. The v12 rollout is staged, and the team lists warm and frozen install optimization as post-v12 work. The RC release notes still land fixes in this exact path (directory-swap races in the shared-store materializer). And the paths pnpm's own headline numbers come from are fast in our table too: the 20 ms repeat install and the 579 ms GVS row. Judge pnpm 12 by those two rows; treat the 11.2 s row as a bug to retest at stable. Upstream tracks the regression as [pnpm #11851](https://github.com/pnpm/pnpm/issues/11851) (syscall contention in the Rust materializer, macOS); our disk-block probe adds the copy-vs-clone evidence to that thread.

## 3. Features

### Verified to work (our tests)

- `pnpm-lock.yaml` v9 roundtrip: byte-identical after `aube install`, on a 2-package workspace and on the 1302-package fixture.
- Workspace `catalog:` specifiers resolve correctly from `pnpm-workspace.yaml`.
- `overrides` apply (forced `ms@2.1.2` appeared in the lockfile and on disk).
- aube edits an existing `pnpm-workspace.yaml` in place. aube created no stray files in the project.
- aube gates build scripts with the pnpm 11 `allowBuilds` model. It adds an optional jail with env, path, and network permissions per package glob.

### Verified broken or divergent (our tests)

| Area | pnpm 11.20 behavior | aube 1.37 behavior | vp impact |
| --- | --- | --- | --- |
| `minimumReleaseAge` | Gates resolution; also rejects too-new lockfile entries under policy | `aube add` picked a 3-day-old nanoid under a 2-year window set in both `pnpm-workspace.yaml` and `.npmrc` | High. vp documents and tests this workflow; a silent bypass weakens our security story |
| Registry env vars | Reads `PNPM_CONFIG_REGISTRY`; ignores `npm_config_registry` / `NPM_CONFIG_REGISTRY` | Exact inverse: reads the npm spellings, ignores `PNPM_CONFIG_*` | Breaks our documented CI recipes when aube impersonates pnpm |
| `add --lockfile-only` | Supported | Rejected with a usage error | Small, but disproves "every pnpm flag" parity |
| Switch-back | n/a | After an aube install, `pnpm add` fails with `ERR_PNPM_HOIST_PATTERN_DIFF` until a full `pnpm install` recreates `node_modules` | "Run both side by side" is true for the lockfile, not for `node_modules` |
| Old lockfiles | Reads v5-v8 | Refuses v5-v8 with an upgrade instruction | `vp migrate` must keep a pnpm fallback for old repos |

aube has its own `minimumReleaseAge` implementation (24h default, `minimumReleaseAgeStrict`, `minimumReleaseAgeExclude`). The vocabulary matches pnpm; the enforcement scope does not. We did not isolate whether only `add` has the gap. The open nub issue [#681](https://github.com/nubjs/nub/issues/681) shows the gate fires for dlx.

### Claimed but not tested by us

We did not test:

- patches and hooks
- `yarn.lock` handling (aube docs say read-write; the nub site says Yarn is read-only)
- npm and bun lockfile fidelity at scale
- Windows behavior
- private registry auth

Include these tests in any pilot.

## 4. Stability

- Age: the engine is under 4 months old, at 37 minor releases. That pace means behavior changes from week to week. aube needed v1.36 for "pnpm 11.18 parity". aube follows pnpm's surface with a lag of days.
- Governance: jdx/aube has issues disabled and a bot that auto-closes PRs. There is no public channel for engine bugs against aube itself. Users report engine bugs in the nub tracker instead. For a vendor-critical dependency, vp needs a support agreement or a public tracker.
- Open bugs in the nub tracker that map to vp scenarios:
  - [#667](https://github.com/nubjs/nub/issues/667): vite-plus was broken under GVS; mitigated now (see below).
  - [#660](https://github.com/nubjs/nub/issues/660): one failed `optionalDependencies` build fails the whole install; npm and pnpm exit 0.
  - [#657](https://github.com/nubjs/nub/issues/657): a false "already up to date" result when the lockfile omits a declared dependency.
  - [#656](https://github.com/nubjs/nub/issues/656): the node shim executes itself and hangs (the bug class we fixed in vp [#1820](https://github.com/voidzero-dev/vite-plus/pull/1820)).
  - [#654](https://github.com/nubjs/nub/issues/654): the engine ignores env aliases for `cacheDir`.
- Issue [#667](https://github.com/nubjs/nub/issues/667), "Can't use with vite+": `@voidzero-dev/vite-plus-core` requires `vite-plus/binding` but does not declare `vite-plus` as a dependency. Under aube/nub's global virtual store, the realpath lives outside the project. Node's upward walk never finds `vite-plus`, and `vp build` crashes. pnpm's project-local virtual store masks the same undeclared dependency today. So the flagship speed feature broke our own published packages. The correct fix is on our side: declare the dependency, or restructure the binding lookup.
- Status update, 2026-08-07. Three changes mitigate #667, and we verified the result locally:
  - vp [PR #2313](https://github.com/voidzero-dev/vite-plus/pull/2313) (shipped in vite-plus 0.2.8) resolves the bundled Rolldown bindings through platform packages. This removes the `vite-plus/binding` require from core.
  - nub 0.7.2 detects undeclared imports and materializes those packages per-project. It flags vite-plus for its optional `playwright` and `webdriverio` imports, so vite-plus stays out of nub's global store.
  - aube 1.37's store layout nests a package's closure inside one slot, so the old require also resolves there.
  - We verified `vp build` passes under nub 0.7.2 with vite-plus 0.2.8, and under aube 1.37's global store with both 0.2.7 and 0.2.8. The issue stays open. vite-plus gets no GVS speed under nub until we declare or remove the optional imports.

## 5. The Rust pnpm alternative

pnpm v12 reached RC on 2026-08-05. Our measurements confirm its direction: 20 ms repeat installs now, and 579 ms warm installs with the global virtual store on. pnpm 11 already ships the same store behind `enableGlobalVirtualStore: true` (experimental) and measures 439 ms warm through vp today, so most of the warm-install win needs no version jump at all. It keeps pnpm's own lockfile semantics, policy engine, and config surface, so it carries no compatibility risk. Its open gaps are the RC regressions and the staged rollout (resolution stays in TypeScript for now). If the motive for aube is speed alone, pnpm 12 with `enableGlobalVirtualStore: true` delivers most of it without an impersonation layer. vp already manages pnpm versions.

pnpm 12 does not give us:

- the 7 ms binary startup for script dispatch
- the embeddable Rust crate (aube ships on crates.io with a Host profile API; our NAPI binding or `vp_global_cli` can link it directly)
- the lifecycle-script jail
- one engine across npm, pnpm, yarn, and bun lockfiles

## 6. Risk assessment for `vp install` on aube by default

1. Correctness risk, medium today. [nub #667](https://github.com/nubjs/nub/issues/667) broke vp's own toolchain under default settings; current engine and vite-plus versions mitigate it (section 4), and old pairs still crash. Min-release-age enforcement diverges. The optional-dep and staleness bugs are open.
2. Compatibility risk, medium. Lockfile fidelity is excellent in our tests. The env-var and flag surfaces have verified gaps. We did not measure Windows.
3. Vendor risk, medium-high. One company, a closed contribution model, no public engine tracker, 4 months of history. The MIT license and the crates.io publication cap the downside: we can fork or pin.
4. Speed claim risk, low-medium. The numbers are real but scenario-shaped. Our CI-heavy users see the GVS-off column, where aube has no lead over pnpm 11 and loses to pnpm 12.
5. Opportunity cost. pnpm 12 stable will close most of the speed gap for pnpm projects, with no migration risk, from a team with seven years of production history.

## 7. Recommended gates before any default switch

1. GVS eligibility for vite-plus. [PR #2313](https://github.com/voidzero-dev/vite-plus/pull/2313) fixed the core binding lookup. Declare or remove the remaining optional imports (`playwright`, `webdriverio`) so engine heuristics stop the exclusion of vite-plus. Verify vp under GVS end to end.
2. Verified min-release-age parity: an aube release must enforce the configured window for add, install, and dlx. Add PTY snapshot coverage on our side.
3. Env contract: `vp install` must keep our documented `PNPM_CONFIG_*` behavior with any engine underneath. Decide whether vp translates the env or requires aube support.
4. A support channel with en.dev for engine bugs, or a reopened public tracker.
5. Windows parity run of our full snapshot suite with the engine flag on.
6. A benchmark rerun against pnpm 12 stable, on Linux CI hardware, before we cite any speed numbers publicly.

Recommendation: introduce the engine behind an explicit opt-in (a config key or an `--engine` flag). Pilot it in ecosystem-ci against the fork catalog. Hold the default flip until the six gates pass. Publish nothing about a default until gate 1 lands. A reviewer's first test is `vp build` on a template. That passes with nub 0.7.2 and vite-plus 0.2.8, but only through engine fallback heuristics. Older engine or vite-plus versions still crash.

## Appendix A: the vp POC (2026-08-07)

We embedded the `aube` crate in `vp_pm_cli` behind `VP_INSTALL_ENGINE=aube` (four files, about 50 lines). The change is on this branch in `crates/vp_pm_cli/src/dispatch.rs`. The six-setup comparison table in section 2, Our measurements, comes from this POC run: same ~1302-package fixture, one shared `pnpm-lock.yaml`, per-setup store isolation, hyperfine with 5 runs.

POC-specific readings:

- The embed path works and keeps the lockfile byte-identical. The nub and standalone-aube setups also return the lockfile untouched.
- Embedded aube (374 ms warm) matches the standalone aube CLI (361 ms) within run noise. The in-process embed adds no measurable cost and removes a process spawn.
- pnpm 12's native binary answers vp's repeat install in 20 ms, about 14x faster than the aube embed path, which revalidates the shared store on every run.
- vp accepts a `packageManager: pnpm@12.0.0-rc.0` pin today, so the pnpm 12 upgrade path needs no vp code change.

## Appendix B: sources and artifacts

- Benchmark scripts: `rfcs/aube-install-engine-evaluation/standalone-matrix.sh` and `rfcs/aube-install-engine-evaluation/vp-poc-matrix.sh`; setup and recorded tool versions in the sibling `README.md`. Raw hyperfine JSON and the compat repro transcripts stayed local.
- aube docs: benchmarks, pnpm-users, settings reference (GVS heuristics at `disableGlobalVirtualStoreForPackages`), embedding guide.
- nub: nubjs.com claims pages, issue tracker ([#667](https://github.com/nubjs/nub/issues/667), [#681](https://github.com/nubjs/nub/issues/681), [#660](https://github.com/nubjs/nub/issues/660), [#657](https://github.com/nubjs/nub/issues/657), [#656](https://github.com/nubjs/nub/issues/656), [#654](https://github.com/nubjs/nub/issues/654)).
- pnpm v12: v12.0.0-rc.0 release notes, official rewrite announcements, morello.dev summary of the phase plan.
