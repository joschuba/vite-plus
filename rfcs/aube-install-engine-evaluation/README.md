# Benchmark scripts for the aube install-engine evaluation

These scripts produced the numbers in `../aube-install-engine-evaluation.md`.

- `standalone-matrix.sh`: aube vs pnpm 11 vs pnpm 12 outside vp. Scenarios: warm, no-op, cold. All arms share one v9 `pnpm-lock.yaml`.
- `vp-poc-matrix.sh`: the vp arms. Managed pnpm 11, managed pnpm 12 (default and with the global virtual store), the embedded-aube POC hook (`VP_INSTALL_ENGINE=aube`), and standalone nub.

Both scripts read their inputs from environment variables and print the required ones when missing. Each run stages isolated per-arm homes, stores, and caches under a temp directory, so nothing touches your real pnpm or aube state.

The recorded numbers came from: macOS arm64 (M-series), Node 24.19.0, registry.npmmirror.com, aube 1.37.0, pnpm 11.20.0, pnpm 12.0.0-rc.0, nub 0.7.2, vp v0.2.8 with the POC hook, on 2026-08-06 and 2026-08-07. The fixture is aube's own benchmark manifest (~1302 packages after resolution): <https://github.com/jdx/aube/blob/main/benchmarks/fixture.package.json>. Registry latency only shapes the cold scenario; warm and no-op scenarios run from local stores.
