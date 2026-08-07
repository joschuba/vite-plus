# Benchmark scripts for the aube install-engine evaluation

These scripts produced the numbers in `../aube-install-engine-evaluation.md`.

- `standalone-matrix.sh`: aube vs pnpm 11 vs pnpm 12 outside vp. Scenarios: warm, repeat (already installed), cold. All setups share one v9 `pnpm-lock.yaml`.
- `vp-poc-matrix.sh`: the eight vp POC setups. Managed pnpm 11, managed pnpm 12 (default and with the global virtual store), the embedded-aube POC hook (`VP_INSTALL_ENGINE=aube`, store on and off), the standalone aube CLI, and standalone nub (store on and off).

Both scripts read their inputs from environment variables and print the required ones when missing. Each run stages isolated homes, stores, and caches for each setup under a temp directory. The run does not touch your real pnpm or aube state.

The recorded numbers came from this setup, on 2026-08-06 and 2026-08-07:

- macOS arm64 (M-series), Node 24.19.0; the cold scenario ran against registry.npmjs.org
- aube 1.37.0, pnpm 11.20.0, pnpm 12.0.0-rc.0, nub 0.7.2
- vp v0.2.8 with the POC hook

The fixture is aube's own benchmark manifest (~1302 packages after resolution): <https://github.com/jdx/aube/blob/main/benchmarks/fixture.package.json>. Registry latency changes only the cold scenario. Warm and repeat scenarios run from local stores.
