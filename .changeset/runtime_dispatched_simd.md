---
sia_reed_solomon: minor
---

# Support NEON, AVX2, GFNI

`encode`, `verify`, and `reconstruct` now use NEON on aarch64, AVX2 or GFNI
on x86_64 (whichever the CPU supports), and the scalar fallback elsewhere.

A new `simd` Cargo feature is on by default. Set `default-features = false`
to force the scalar path (useful for WASM, benchmarking, or debugging a
suspected SIMD bug).

Apple M-series, 10-of-30 @ 4 MiB shards:

| Operation                | Before    | After       | Speedup |
|--------------------------|-----------|-------------|---------|
| `encode`                 | 6.3 GiB/s | 61.9 GiB/s  | 9.8x    |
| `verify`                 | 5.8 GiB/s | 29.8 GiB/s  | 5.1x    |
| `reconstruct -1 data`    | 34.2 GiB/s| 110.6 GiB/s | 3.2x    |
| `reconstruct -10 data`   | 4.1 GiB/s | 31.3 GiB/s  | 7.6x    |

AMD EPYC 7B13 (AVX2):

| Operation                | Before    | After      | Speedup |
|--------------------------|-----------|------------|---------|
| `encode`                 | 6.8 GiB/s | 24.4 GiB/s | 3.6x    |
| `verify`                 | 3.2 GiB/s | 4.3 GiB/s  | 1.3x    |
| `reconstruct -1 data`    | 1.8 GiB/s | 7.9 GiB/s  | 4.3x    |
| `reconstruct -10 data`   | 3.4 GiB/s | 4.4 GiB/s  | 1.3x    |
