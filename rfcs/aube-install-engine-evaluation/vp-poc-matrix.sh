#!/usr/bin/env bash
# vp POC matrix, eight setups:
#   vp + managed pnpm 11 / 12 / 12+GVS, vp + embedded aube (GVS on and off),
#   standalone aube CLI, standalone nub (GVS on and off).
#
# Prerequisites:
#   - hyperfine on PATH
#   - a vp binary built with the aube embed POC
#     (VP_INSTALL_ENGINE=aube hook in crates/vp_pm_cli/src/dispatch.rs)
#   - an aube binary (mise/npm/cargo, see aube docs)
#   - a nub binary (release tarball from https://github.com/nubjs/nub/releases)
#   - FIXTURE_DIR contains package.json and a v9 pnpm-lock.yaml
#
# The pnpm setups run through vp's managed package-manager path. The pnpm 12
# setups pin `packageManager: pnpm@<PNPM12_VERSION>` in package.json, and vp
# provisions that version itself. nub runs standalone because vp rejects
# `devEngines.packageManager: nub`.
#
# GVS toggles: the embedded aube honors `enableGlobalVirtualStore: false` in
# pnpm-workspace.yaml. nub 0.7.2 ignores that key there and needs
# `enable-global-virtual-store=false` in .npmrc, so the nub-off setup sets both.
#
# Overridable environment:
#   FIXTURE_DIR     (required)
#   VP_BIN          vp binary with the POC hook (required)
#   AUBE_BIN        aube binary (required)
#   NUB_BIN         nub binary (required)
#   POC_DIR         work area                 (default: mktemp -d)
#   PNPM12_VERSION  pnpm 12 pin               (default: 12.0.0-rc.0)
#   REGISTRY        npm registry              (default: https://registry.npmjs.org/)
#   RUNS                                      (default: 5)
set -euo pipefail

FIXTURE_DIR=${FIXTURE_DIR:?set FIXTURE_DIR to a dir with package.json + pnpm-lock.yaml}
VP=${VP_BIN:?set VP_BIN to a vp binary built with the aube POC}
AUBE=${AUBE_BIN:?set AUBE_BIN to an aube binary}
NUB=${NUB_BIN:?set NUB_BIN to a nub binary}
P=${POC_DIR:-$(mktemp -d /tmp/vp-aube-poc.XXXXXX)}
PNPM12_VERSION=${PNPM12_VERSION:-12.0.0-rc.0}
REG=${REGISTRY:-https://registry.npmjs.org/}
RUNS=${RUNS:-5}

AUBE_ENV="HOME=$HOME XDG_CACHE_HOME=$P/aube-xdg/cache XDG_DATA_HOME=$P/aube-xdg/data VP_INSTALL_ENGINE=aube"
AUBECLI_ENV="HOME=$HOME XDG_CACHE_HOME=$P/aubecli-xdg/cache XDG_DATA_HOME=$P/aubecli-xdg/data"
NUB_ENV="HOME=$HOME XDG_CACHE_HOME=$P/nub-xdg/cache XDG_DATA_HOME=$P/nub-xdg/data"

setups=(pnpm11 pnpm12 pnpm12gvs aube aubecli aube-off nub nub-off)

stage() {
  local t=$1
  mkdir -p "$P/$t" "$P/stores/$t"
  cp "$FIXTURE_DIR/package.json" "$P/$t/package.json"
  cp "$FIXTURE_DIR/pnpm-lock.yaml" "$P/$t/pnpm-lock.yaml"
  printf 'registry=%s\n' "$REG" > "$P/$t/.npmrc"
  # allowBuilds: pnpm 11 aborts on unreviewed build scripts. Deny them in
  # every setup so all engines skip builds the same way. Extend the list if
  # your fixture reports more packages.
  printf 'storeDir: %s/stores/%s/store\ncacheDir: %s/stores/%s/cache\nallowBuilds:\n  "@parcel/watcher": false\n' "$P" "$t" "$P" "$t" > "$P/$t/pnpm-workspace.yaml"
}

pin_pnpm12() {
  node -e "const f=process.argv[1],j=require(f);j.packageManager='pnpm@$PNPM12_VERSION';require('fs').writeFileSync(f,JSON.stringify(j,null,2)+'\n')" "$P/$1/package.json"
}

echo "== stage in $P =="
for t in "${setups[@]}"; do stage "$t"; done
pin_pnpm12 pnpm12
pin_pnpm12 pnpm12gvs
printf 'enableGlobalVirtualStore: true\n' >> "$P/pnpm12gvs/pnpm-workspace.yaml"
printf 'enableGlobalVirtualStore: false\n' >> "$P/aube-off/pnpm-workspace.yaml"
printf 'enableGlobalVirtualStore: false\n' >> "$P/nub-off/pnpm-workspace.yaml"
printf 'enable-global-virtual-store=false\n' >> "$P/nub-off/.npmrc"

label() {
  case $1 in
    pnpm11)    echo "vp + managed pnpm 11" ;;
    pnpm12)    echo "vp + managed pnpm $PNPM12_VERSION" ;;
    pnpm12gvs) echo "vp + managed pnpm $PNPM12_VERSION + GVS" ;;
    aube)      echo "vp + embedded aube (GVS on)" ;;
    aubecli)   echo "aube CLI (standalone, GVS on)" ;;
    aube-off)  echo "vp + embedded aube (GVS off)" ;;
    nub)       echo "nub (standalone, GVS on)" ;;
    nub-off)   echo "nub (standalone, GVS off)" ;;
  esac
}
cmd() {
  case $1 in
    aube|aube-off) echo "cd $P/$1 && env $AUBE_ENV $VP install" ;;
    aubecli)       echo "cd $P/$1 && env $AUBECLI_ENV $AUBE install" ;;
    nub|nub-off)   echo "cd $P/$1 && env $NUB_ENV $NUB install" ;;
    *)             echo "cd $P/$1 && $VP install" ;;
  esac
}

echo "== prime =="
for t in "${setups[@]}"; do
  bash -c "$(cmd "$t")" > "$P/prime-$t.log" 2>&1 || { echo "PRIME-$t-FAILED"; tail -8 "$P/prime-$t.log"; exit 1; }
done

echo "== lockfile drift check =="
for t in aube aubecli nub; do
  diff -q "$FIXTURE_DIR/pnpm-lock.yaml" "$P/$t/pnpm-lock.yaml" > /dev/null && echo "$t: lock identical" || echo "$t: LOCK CHANGED"
done

echo "== placement check (expect project-local for the -off setups) =="
for t in aube-off nub-off; do
  rp=$(node -p "require('fs').realpathSync('$P/$t/node_modules/typescript')")
  case "$rp" in
    *virtual-store*|*/pm/store/*) echo "$t: GLOBAL STORE (toggle did not apply)" ;;
    *) echo "$t: project-local" ;;
  esac
done

W=""
for t in "${setups[@]}"; do W="$W $P/$t/node_modules"; done

warm_args=(); repeat_args=()
for t in "${setups[@]}"; do
  warm_args+=(-n "$(label "$t")" "$(cmd "$t")")
  repeat_args+=(-n "$(label "$t")" "$(cmd "$t")")
done

echo "== warm install (node_modules wiped each run) =="
hyperfine --warmup 1 --runs "$RUNS" --export-json "$P/warm.json" --prepare "rm -rf $W" "${warm_args[@]}"

echo "== repeat install (already installed) =="
hyperfine --warmup 1 --runs "$RUNS" --export-json "$P/noop.json" "${repeat_args[@]}"

echo "== done: results in $P/{warm,noop}.json =="
