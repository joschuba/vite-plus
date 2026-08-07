#!/usr/bin/env bash
# Standalone engine matrix: aube vs pnpm 11 vs pnpm 12 on one shared
# pnpm-lock.yaml (the vp impersonation scenario).
#
# Scenarios: warm (store kept, node_modules wiped), no-op, cold (all wiped).
# Arms: pnpm11, pnpm12, pnpm12+global-virtual-store, aube, aube without GVS.
#
# Prerequisites:
#   - hyperfine on PATH
#   - FIXTURE_DIR contains package.json and a v9 pnpm-lock.yaml.
#     The report used aube's own fixture (~1302 packages):
#     https://github.com/jdx/aube/blob/main/benchmarks/fixture.package.json
#     with the lockfile generated once by `pnpm install --lockfile-only`.
#
# Overridable environment:
#   FIXTURE_DIR  (required)
#   BENCH_DIR    work area                  (default: mktemp -d)
#   AUBE_BIN     aube binary                (default: aube on PATH)
#   PNPM11_BIN   pnpm 11 binary             (default: pnpm on PATH)
#   PNPM12_BIN   pnpm 12 binary             (required for the pnpm12 arms)
#   REGISTRY     npm registry               (default: https://registry.npmjs.org/)
#   RUNS/COLD_RUNS                          (default: 5 / 3)
set -euo pipefail

FIXTURE_DIR=${FIXTURE_DIR:?set FIXTURE_DIR to a dir with package.json + pnpm-lock.yaml}
B=${BENCH_DIR:-$(mktemp -d /tmp/vp-aube-bench.XXXXXX)}
AUBE=${AUBE_BIN:-aube}
PNPM11=${PNPM11_BIN:-pnpm}
PNPM12=${PNPM12_BIN:?set PNPM12_BIN to a pnpm 12 binary}
REG=${REGISTRY:-https://registry.npmjs.org/}
RUNS=${RUNS:-5}
COLD_RUNS=${COLD_RUNS:-3}

tools=(pnpm11 pnpm12 pnpm12gvs aube aubenogvs)

setup_proj() {
  local t=$1
  mkdir -p "$B/proj-$t" "$B/homes/$t" "$B/caches/$t"
  cp "$FIXTURE_DIR/package.json" "$B/proj-$t/package.json"
  cp "$FIXTURE_DIR/pnpm-lock.yaml" "$B/proj-$t/pnpm-lock.yaml"
  printf 'registry=%s\n' "$REG" > "$B/proj-$t/.npmrc"
  case $t in
    pnpm11|pnpm12)
      printf 'storeDir: %s/caches/%s/store\ncacheDir: %s/caches/%s/cache\n' "$B" "$t" "$B" "$t" > "$B/proj-$t/pnpm-workspace.yaml" ;;
    pnpm12gvs)
      printf 'storeDir: %s/caches/%s/store\ncacheDir: %s/caches/%s/cache\nenableGlobalVirtualStore: true\n' "$B" "$t" "$B" "$t" > "$B/proj-$t/pnpm-workspace.yaml" ;;
  esac
}

cmd_for() {
  local t=$1
  local h="$B/homes/$t" c="$B/caches/$t" d="$B/proj-$t"
  case $t in
    pnpm11)   echo "cd $d && env HOME=$h XDG_CACHE_HOME=$c $PNPM11 install --ignore-scripts --no-frozen-lockfile" ;;
    pnpm12)   echo "cd $d && env HOME=$h XDG_CACHE_HOME=$c $PNPM12 install --ignore-scripts --no-frozen-lockfile" ;;
    pnpm12gvs) echo "cd $d && env HOME=$h XDG_CACHE_HOME=$c $PNPM12 install --ignore-scripts --no-frozen-lockfile" ;;
    aube)     echo "cd $d && env HOME=$h XDG_CACHE_HOME=$c XDG_DATA_HOME=$h/.local/share $AUBE install" ;;
    aubenogvs) echo "cd $d && env HOME=$h XDG_CACHE_HOME=$c XDG_DATA_HOME=$h/.local/share $AUBE install --disable-global-virtual-store" ;;
  esac
}

wipe_nm="rm -rf"
for t in "${tools[@]}"; do wipe_nm="$wipe_nm $B/proj-$t/node_modules"; done

echo "== setup in $B =="
for t in "${tools[@]}"; do setup_proj "$t"; done

echo "== prime stores (one full install per tool) =="
for t in "${tools[@]}"; do
  echo "-- prime $t"
  bash -c "$(cmd_for "$t")" > "$B/prime-$t.log" 2>&1 || { echo "PRIME FAILED: $t"; tail -5 "$B/prime-$t.log"; }
done

echo "== scenario: warm (store warm, node_modules wiped) =="
hyperfine --warmup 1 --runs "$RUNS" --export-json "$B/warm.json" \
  --prepare "$wipe_nm" \
  -n pnpm11 "$(cmd_for pnpm11)" \
  -n pnpm12 "$(cmd_for pnpm12)" \
  -n pnpm12gvs "$(cmd_for pnpm12gvs)" \
  -n aube "$(cmd_for aube)" \
  -n aubenogvs "$(cmd_for aubenogvs)"

echo "== scenario: noop (already installed) =="
hyperfine --warmup 1 --runs "$RUNS" --export-json "$B/noop.json" \
  -n pnpm11 "$(cmd_for pnpm11)" \
  -n pnpm12 "$(cmd_for pnpm12)" \
  -n aube "$(cmd_for aube)"

echo "== scenario: cold (stores + caches + node_modules wiped) =="
hyperfine --runs "$COLD_RUNS" --export-json "$B/cold.json" \
  --prepare "$wipe_nm && rm -rf $B/caches/pnpm11 $B/caches/pnpm12 $B/caches/aube $B/homes/aube/.local/share" \
  -n pnpm11 "$(cmd_for pnpm11)" \
  -n pnpm12 "$(cmd_for pnpm12)" \
  -n aube "$(cmd_for aube)"

echo "== done: results in $B/{warm,noop,cold}.json =="
