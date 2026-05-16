# Changelog

## 0.3.0 (2026-05-16)

### Breaking Changes

#### Add SIMD128 support to WASM32 target

New `simd128` backend on `wasm32-unknown-unknown`, using the same nibble-split as the NEON path. 
Enabled via the `simd` feature (on by default) when compiled with `-C target-feature=+simd128`.

On c7i.4xlarge, V8 (Node), 4 MiB shards, 10+20 Vandermonde:

| Operation                 | wasm scalar | wasm + simd128 | Δ    |
|---------------------------|-------------|----------------|------|
| `encode`                  | 337 MiB/s   | 1.7 GiB/s      | 5.0× |
| `verify`                  | 308 MiB/s   | 1.1 GiB/s      | 3.7× |
| `reconstruct -1 data`     | 1.1 GiB/s   | 1.7 GiB/s      | 1.6× |
| `reconstruct -10 data`    | 205 MiB/s   | 729 MiB/s      | 3.6× |

### Features

#### Improve scalar fallback performance

8× unrolled `mul_slice` / `mul_slice_xor`. On c7i.4xlarge,
`--no-default-features --features parallel` (16 vCPU scalar with rayon):

| Operation                 | before     | after      | Δ    |
|---------------------------|------------|------------|------|
| `encode`                  | 3.4 GiB/s  | 4.6 GiB/s  | +35% |
| `verify`                  | 2.1 GiB/s  | 2.6 GiB/s  | +24% |
| `reconstruct -1 data`     | 13.4 GiB/s | 17.4 GiB/s | +30% |
| `reconstruct -10 data`    | 2.0 GiB/s  | 2.7 GiB/s  | +35% |

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

## v0.1.0 (2026-05-13)

- Initial Release
