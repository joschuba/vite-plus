#!/usr/bin/env bash
# vp POC matrix: managed pnpm 11 / 12 / 12+GVS, embedded aube, standalone nub.
#
# Prerequisites:
#   - hyperfine on PATH
#   - a vp binary built with the aube embed POC
#     (VP_INSTALL_ENGINE=aube hook in crates/vp_pm_cli/src/dispatch.rs)
#   - a nub binary (release tarball from https://github.com/nubjs/nub/releases)
#   - FIXTURE_DIR contains package.json and a v9 pnpm-lock.yaml
#
# The pnpm arms run through vp's managed package-manager path. The pnpm 12
# arms pin `packageManager: pnpm@<PNPM12_VERSION>` in package.json, and vp
# provisions that version itself. nub runs standalone because vp rejects
# `devEngines.packageManager: nub`.
#
# Overridable environment:
#   FIXTURE_DIR     (required)
#   VP_BIN          vp binary with the POC hook (required)
#   NUB_BIN         nub binary (required)
#   POC_DIR         work area                 (default: mktemp -d)
#   PNPM12_VERSION  pnpm 12 pin               (default: 12.0.0-rc.0)
#   REGISTRY        npm registry              (default: https://registry.npmjs.org/)
#   RUNS                                      (default: 5)
set -euo pipefail

FIXTURE_DIR=${FIXTURE_DIR:?set FIXTURE_DIR to a dir with package.json + pnpm-lock.yaml}
VP=${VP_BIN:?set VP_BIN to a vp binary built with the aube POC}
NUB=${NUB_BIN:?set NUB_BIN to a nub binary}
P=${POC_DIR:-$(mktemp -d /tmp/vp-aube-poc.XXXXXX)}
PNPM12_VERSION=${PNPM12_VERSION:-12.0.0-rc.0}
REG=${REGISTRY:-https://registry.npmjs.org/}
RUNS=${RUNS:-5}

AUBE_ENV="HOME=$HOME XDG_CACHE_HOME=$P/aube-xdg/cache XDG_DATA_HOME=$P/aube-xdg/data VP_INSTALL_ENGINE=aube"
NUB_ENV="HOME=$HOME XDG_CACHE_HOME=$P/nub-xdg/cache XDG_DATA_HOME=$P/nub-xdg/data"

stage() {
  local t=$1
  mkdir -p "$P/$t" "$P/stores/$t"
  cp "$FIXTURE_DIR/package.json" "$P/$t/package.json"
  cp "$FIXTURE_DIR/pnpm-lock.yaml" "$P/$t/pnpm-lock.yaml"
  printf 'registry=%s\n' "$REG" > "$P/$t/.npmrc"
  # allowBuilds: pnpm 11 aborts on unreviewed build scripts. Deny them in
  # every arm so all engines skip builds the same way. Extend the list if
  # your fixture reports more packages.
  printf 'storeDir: %s/stores/%s/store\ncacheDir: %s/stores/%s/cache\nallowBuilds:\n  "@parcel/watcher": false\n' "$P" "$t" "$P" "$t" > "$P/$t/pnpm-workspace.yaml"
}

pin_pnpm12() {
  node -e "const f=process.argv[1],j=require(f);j.packageManager='pnpm@$PNPM12_VERSION';require('fs').writeFileSync(f,JSON.stringify(j,null,2)+'\n')" "$P/$1/package.json"
}

echo "== stage in $P =="
for t in pnpm11 pnpm12 pnpm12gvs aube nub; do stage "$t"; done
pin_pnpm12 pnpm12
pin_pnpm12 pnpm12gvs
printf 'enableGlobalVirtualStore: true\n' >> "$P/pnpm12gvs/pnpm-workspace.yaml"

echo "== prime =="
(cd "$P/pnpm11" && $VP install) > "$P/prime-pnpm11.log" 2>&1 || { echo PRIME-PNPM11-FAILED; tail -6 "$P/prime-pnpm11.log"; exit 1; }
(cd "$P/pnpm12" && $VP install) > "$P/prime-pnpm12.log" 2>&1 || { echo PRIME-PNPM12-FAILED; tail -6 "$P/prime-pnpm12.log"; exit 1; }
(cd "$P/pnpm12gvs" && $VP install) > "$P/prime-pnpm12gvs.log" 2>&1 || { echo PRIME-12GVS-FAILED; tail -6 "$P/prime-pnpm12gvs.log"; exit 1; }
(cd "$P/aube" && env $AUBE_ENV $VP install) > "$P/prime-aube.log" 2>&1 || { echo PRIME-AUBE-FAILED; tail -8 "$P/prime-aube.log"; exit 1; }
(cd "$P/nub" && env $NUB_ENV $NUB install) > "$P/prime-nub.log" 2>&1 || { echo PRIME-NUB-FAILED; tail -8 "$P/prime-nub.log"; exit 1; }

echo "== lockfile drift check (aube and nub arms) =="
diff -q "$FIXTURE_DIR/pnpm-lock.yaml" "$P/aube/pnpm-lock.yaml" && echo aube-lock-identical || echo AUBE-LOCK-CHANGED
diff -q "$FIXTURE_DIR/pnpm-lock.yaml" "$P/nub/pnpm-lock.yaml" && echo nub-lock-identical || echo NUB-LOCK-CHANGED

W="rm -rf $P/pnpm11/node_modules $P/pnpm12/node_modules $P/pnpm12gvs/node_modules $P/aube/node_modules $P/nub/node_modules"

echo "== warm install (node_modules wiped each run) =="
hyperfine --warmup 1 --runs "$RUNS" --export-json "$P/warm.json" --prepare "$W" \
  -n "vp + managed pnpm 11" "cd $P/pnpm11 && $VP install" \
  -n "vp + managed pnpm $PNPM12_VERSION" "cd $P/pnpm12 && $VP install" \
  -n "vp + managed pnpm $PNPM12_VERSION + GVS" "cd $P/pnpm12gvs && $VP install" \
  -n "vp + embedded aube" "cd $P/aube && env $AUBE_ENV $VP install" \
  -n "nub (standalone)" "cd $P/nub && env $NUB_ENV $NUB install"

echo "== no-op install =="
hyperfine --warmup 1 --runs "$RUNS" --export-json "$P/noop.json" \
  -n "vp + managed pnpm 11" "cd $P/pnpm11 && $VP install" \
  -n "vp + managed pnpm $PNPM12_VERSION" "cd $P/pnpm12 && $VP install" \
  -n "vp + managed pnpm $PNPM12_VERSION + GVS" "cd $P/pnpm12gvs && $VP install" \
  -n "vp + embedded aube" "cd $P/aube && env $AUBE_ENV $VP install" \
  -n "nub (standalone)" "cd $P/nub && env $NUB_ENV $NUB install"

echo "== done: results in $P/{warm,noop}.json =="
