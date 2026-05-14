## v0.1.0 (2026-05-13)

- Initial Release
## 0.2.0 (2026-05-14)

### Breaking Changes

#### Support NEON, AVX2, GFNI

`encode`, `verify`, and `reconstruct` now use NEON on aarch64, AVX2 or GFNI
on x86_64 (whichever the CPU supports), and a no-SIMD fallback elsewhere.

A new `simd` Cargo feature is on by default. Set `default-features = false`
to force the no-SIMD path (useful for WASM, benchmarking, or debugging a
suspected SIMD bug).

10-of-30 @ 4 MiB shards, AWS `c7i.4xlarge` (GFNI):

| Operation                | Before     | After      | Speedup |
|--------------------------|------------|------------|---------|
| `encode`                 | 3.4 GiB/s  | 26.5 GiB/s | 7.9x    |
| `verify`                 | 2.1 GiB/s  | 4.7 GiB/s  | 2.2x    |
| `reconstruct -1 data`    | 13.4 GiB/s | 33.5 GiB/s | 2.5x    |
| `reconstruct -10 data`   | 2.0 GiB/s  | 8.3 GiB/s  | 4.2x    |

### Features

- Skip rayon when per-thread work would be too small to amortize contention.
