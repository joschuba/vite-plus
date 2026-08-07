# Internal report: aube as the `vp install` engine

Status: internal, pre-decision. Written 2026-08-06; POC appendix added 2026-08-07.
Scope: evaluate the speed, feature, and stability claims of [aube](https://github.com/jdx/aube) and [nub](https://nubjs.com/), compare them with pnpm 11 and with the Rust pnpm (pnpm v12), and assess the risk of a default switch for `vp install`.

## Verdict, in four sentences

The aube speed claims reproduce on our hardware, but the win comes from a store-layout default (the global virtual store), not from Rust. With that layout normalized, aube is on par with pnpm 11, and the Rust pnpm 12 RC lands within 1.4x of aube. Feature compatibility with pnpm is strong at the lockfile layer and broken at the policy and environment layers; one open bug breaks vite-plus itself under aube's default store. Ship aube only as an opt-in engine for now; the default switch needs the gate list at the end of this report.

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

aube is the engine: resolver, linker, store, lockfile codecs, script runner. nub is one embedder; it wires aube behind a pnpm-flag-compatible CLI. The proposed vp model copies the nub model, so most nub findings transfer directly.

pnpm v12 is the pnpm team's own Rust rewrite. Phase 1 moves fetch and link to Rust; TypeScript still owns resolution and the lockfile write. The RC shipped on 2026-08-05. Kochan states v12 keeps v11 behavior.

## 2. Speed

### Published claims

- aube: warm install 7x faster than pnpm, repeat test commands up to 48x faster (aube 1.37 vs pnpm 11.18, hermetic registry).
- nub: warm frozen install 10x faster than pnpm (346 ms vs 3,453 ms, 1,168 packages).
- pnpm v12: warm install 381 ms down to 12 ms vs v11, clean install 6.5 s down to 2.2 s (official, July 2026).

### Our measurements

Setup: macOS arm64, Node 24.19.0, live npmmirror registry, aube's own ~1302-package benchmark fixture, one shared `pnpm-lock.yaml` (v9) for every tool, per-tool HOME and store isolation, hyperfine. This mirrors the aube methodology with one deliberate change: aube runs on the pnpm lockfile, which is the vp impersonation scenario. GVS = global virtual store.

| Scenario | aube (GVS on, default) | pnpm 12 RC + GVS | pnpm 11 (default) | aube, GVS off | pnpm 12 RC (default) |
| --- | --- | --- | --- | --- | --- |
| Warm install, `node_modules` wiped | **371 ms** | 529 ms | 2.65 s | 3.08 s | 9.72 s |
| No-op `install` on installed tree | 290 ms | n/a | 200 ms | n/a | **10 ms** |
| CLI startup (`--version`) | **6.7 ms** | n/a | 177 ms | n/a | n/a |

Cold install (store, cache, and `node_modules` wiped; live network, 3 runs): pnpm 11 at 34.7 s, pnpm 12 RC at 45.0 s, aube at 50.9 s. aube is the slowest tool cold, 1.47x behind pnpm 11. aube's own hermetic-registry table shows the same shape: cold aube (6.71 s) trails pnpm (6.65 s) and bun (1.45 s). The store architecture that wins warm installs buys nothing on first download.

### What the numbers mean

- The 7x warm claim reproduces: we measured 7.13x vs pnpm 11 at default settings. The claim is honest and their docs state the caveat themselves.
- The cause is the store model, not the language. aube links `node_modules` out of a shared, pre-materialized virtual store. With GVS off, aube (3.08 s) is at pnpm 11 level (2.65 s). pnpm 12 with its own GVS enabled (529 ms) sits 1.4x behind aube.
- CI does not get the headline number. aube forces per-project materialization when `CI=1`, so the CI-relevant comparison is the GVS-off column, where aube holds no lead over pnpm 11.
- The GVS default also self-disables for projects that depend on `next`, `nuxt`, or `parcel` (module-resolution walk-up breaks through the shared store). Those projects get the slow column by design.
- aube's published 7 ms "already installed" number belongs to its `run`-path short circuit, not to `aube install`. Plain no-op `aube install` (290 ms) is slower than pnpm 11 (200 ms), and the pnpm 12 RC wins this scenario outright at 10 ms.
- The pnpm 12 RC default (non-GVS) warm result (9.7 s, syscall-heavy) is an RC-quality regression on macOS. Retest it at stable.
- Binary startup matters for `vp run`-style dispatch: aube starts 26x faster than pnpm's JS bootstrap. vp already pays this cost once per managed-pnpm invocation today.

## 3. Features

### Verified to work (our tests)

- `pnpm-lock.yaml` v9 roundtrip: byte-identical after `aube install`, on a 2-package workspace and on the 1302-package fixture.
- Workspace `catalog:` specifiers resolve correctly from `pnpm-workspace.yaml`.
- `overrides` apply (forced `ms@2.1.2` appeared in the lockfile and on disk).
- An existing `pnpm-workspace.yaml` is respected in place; aube created no stray files in the project.
- Build-script gating follows the pnpm 11 `allowBuilds` model, with an extra optional jail (env, path, network permissions per package glob).

### Verified broken or divergent (our tests)

| Area | pnpm 11.20 behavior | aube 1.37 behavior | vp impact |
| --- | --- | --- | --- |
| `minimumReleaseAge` | Gates resolution; also rejects too-new lockfile entries under policy | `aube add` picked a 3-day-old nanoid under a 2-year window set in both `pnpm-workspace.yaml` and `.npmrc` | High. vp documents and tests this workflow; silent bypass is a security-story regression |
| Registry env vars | Reads `PNPM_CONFIG_REGISTRY`; ignores `npm_config_registry` / `NPM_CONFIG_REGISTRY` | Exact inverse: reads the npm spellings, ignores `PNPM_CONFIG_*` | Breaks our documented CI recipes when aube impersonates pnpm |
| `add --lockfile-only` | Supported | Rejected with a usage error | Small, but disproves "every pnpm flag" parity |
| Switch-back | n/a | After an aube install, `pnpm add` fails with `ERR_PNPM_HOIST_PATTERN_DIFF` until a full `pnpm install` recreates `node_modules` | "Run both side by side" is true for the lockfile, not for `node_modules` |
| Old lockfiles | Reads v5-v8 | Refuses v5-v8 with an upgrade instruction | `vp migrate` must keep a pnpm fallback for old repos |

aube has its own `minimumReleaseAge` implementation (24h default, `minimumReleaseAgeStrict`, `minimumReleaseAgeExclude`), so the vocabulary matches pnpm; the enforcement scope does not. We did not bisect whether the gap is add-time-only. The open nub issue #681 (dlx blocked by the same gate) shows the gate does fire elsewhere.

### Claimed but not tested by us

Patches, hooks, `yarn.lock` handling (aube docs say read-write, the nub site says Yarn is read-only), npm and bun lockfile fidelity at scale, Windows behavior, private registry auth. Budget these into any pilot.

## 4. Stability

- Age: the engine is under 4 months old, at 37 minor releases. The pace is impressive and also means behavior moves week to week. v1.36 was needed for "pnpm 11.18 parity": aube chases pnpm's surface with a lag of days.
- Governance: jdx/aube has issues disabled and a bot that auto-closes PRs. There is no public channel to report an engine bug against aube itself; the nub tracker absorbs them. For a vendor-critical dependency, vp would want a support agreement or at least a reopened tracker.
- Known open bugs in the nub tracker that map to vp scenarios: vite-plus broken under GVS (#667, see below), a failed `optionalDependencies` build fails the whole install where npm/pnpm exit 0 (#660), false "already up to date" when the lockfile omits a declared dependency (#657), node shim self-exec hang (#656, the same bug class we fixed in vp #1820), env aliases for `cacheDir` ignored (#654).
- Issue #667, "Can't use with vite+": `@voidzero-dev/vite-plus-core` requires `vite-plus/binding` but does not declare `vite-plus` as a dependency. Under aube/nub's global virtual store the realpath lives outside the project, Node's upward walk never finds `vite-plus`, and `vp build` crashes. pnpm's project-local virtual store masks the same undeclared dependency today. So the flagship speed feature breaks our own published packages, and the correct fix is on our side (declare the dependency or restructure the binding lookup), with `disableGlobalVirtualStoreForPackages` as the stopgap aube already ships for next/nuxt/parcel.

## 5. The Rust pnpm alternative

pnpm v12 hit RC yesterday. Our measurements confirm its direction: 10 ms no-op installs now, 529 ms warm installs with the global virtual store enabled, with pnpm's own lockfile semantics, policy engine, and config surface at zero compatibility risk. Its remaining gaps are the RC regressions and the phased rollout (resolution still in TypeScript). If the motive for aube is speed alone, pnpm 12 + `enableGlobalVirtualStore: true` delivers most of it without an impersonation layer, and vp already manages pnpm versions.

What pnpm 12 does not give us: the 7 ms binary startup for script dispatch, the embeddable Rust crate (aube publishes `aube` on crates.io with a Host profile API that our NAPI binding or `vp_global_cli` could link directly), the lifecycle-script jail, or one engine across npm/pnpm/yarn/bun lockfiles.

## 6. Risk assessment for `vp install` on aube by default

1. Correctness risk, high today: #667 breaks vp's own toolchain under default settings; min-release-age enforcement diverges; optional-dep and staleness bugs are open.
2. Compatibility risk, medium: lockfile fidelity looks excellent; the env-var and flag surfaces have verified gaps; Windows is unmeasured.
3. Vendor risk, medium-high: single company, closed contribution model, no public engine tracker, 4 months of history. The MIT license and the crates.io publication cap the downside (we can fork or pin).
4. Speed claim risk, low-medium: the numbers are real but scenario-shaped. Our CI-heavy users see the GVS-off column, where aube has no lead over pnpm 11 and loses to pnpm 12.
5. Opportunity cost: pnpm 12 stable will close most of the speed gap for pnpm projects with zero migration risk, on a team with seven years of production history.

## 7. Recommended gates before any default switch

1. Fix `@voidzero-dev/vite-plus-core` to declare its `vite-plus` binding dependency; verify vp under GVS end to end.
2. Verified min-release-age parity: an aube release must enforce the configured window for add, install, and dlx; add PTY snapshot coverage on our side.
3. Env contract: `vp install` must keep our documented `PNPM_CONFIG_*` behavior whatever engine runs underneath; decide whether vp translates the env or requires aube support.
4. A support channel with en.dev for engine bugs, or a reopened public tracker.
5. Windows parity run of our full snapshot suite with the engine flag on.
6. A benchmark rerun against pnpm 12 stable, on Linux CI hardware, before we cite any speed numbers publicly.

Recommendation: introduce the engine behind an explicit opt-in (config key or `--engine` flag), pilot it in ecosystem-ci against the fork catalog, and hold the default flip until the six gates pass. Publish nothing about a default until gate 1 lands, because the first thing a reviewer will try is `vp build` on a template, which fails today under nub.

## Appendix A: vp POC measurements (2026-08-07)

We embedded the `aube` crate in `vp_pm_cli` behind `VP_INSTALL_ENGINE=aube` (four files, about 50 lines; the change rides on this branch, see `crates/vp_pm_cli/src/dispatch.rs`). The six-arm rerun uses the same ~1302-package fixture, one shared `pnpm-lock.yaml`, per-arm store isolation, and hyperfine with 5 runs. The pnpm arms run through vp's managed package-manager path; nub runs standalone because vp rejects `devEngines.packageManager: nub`.

| Arm | Warm install | No-op install |
| --- | --- | --- |
| vp + embedded aube 1.37 (GVS on) | **409 ms** | 295 ms |
| nub 0.7.2 standalone (aube engine) | 524 ms | 114 ms |
| vp + managed pnpm 12.0.0-rc.0 + GVS | 591 ms | **20 ms** |
| vp + managed pnpm 11.20 | 2.70 s | 198 ms |
| vp + managed pnpm 12.0.0-rc.0 (default) | 11.27 s | 20 ms |

Readings:

- The embed path works and keeps the lockfile byte-identical; the nub arm also roundtrips the lockfile untouched.
- vp + aube wins warm installs, but the margin over pnpm 12 + GVS through the same vp dispatch is 1.45x, not 7x.
- pnpm 12's native binary turns vp's no-op install into 20 ms, 15x faster than the aube embed path, which revalidates the shared store every run. nub hides that cost behind its own staleness check (114 ms), which vp could copy for either engine.
- The pnpm 12 RC default (non-GVS) regression from section 2 reproduces through vp (11.3 s, syscall-heavy). Track it against the RC; it should not survive to stable.
- vp accepts a `packageManager: pnpm@12.0.0-rc.0` pin today, so the pnpm 12 upgrade path needs no vp code change.

## Appendix B: sources and artifacts

- Benchmark scripts: `rfcs/aube-install-engine-evaluation/standalone-matrix.sh` and `rfcs/aube-install-engine-evaluation/vp-poc-matrix.sh`; setup and recorded tool versions in the sibling `README.md`. Raw hyperfine JSON and the compat repro transcripts stayed local.
- aube docs: benchmarks, pnpm-users, settings reference (GVS heuristics at `disableGlobalVirtualStoreForPackages`), embedding guide.
- nub: nubjs.com claims pages, issue tracker (#667, #681, #660, #657, #656, #654).
- pnpm v12: v12.0.0-rc.0 release notes, official rewrite announcements, morello.dev summary of the phase plan.
