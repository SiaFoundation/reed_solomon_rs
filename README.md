# Reed Solomon

Reed-Solomon erasure coding over GF(2^8) for the Sia SDK.

A Rust port of the in-memory parts of Klaus Post's
[reedsolomon](https://github.com/klauspost/reedsolomon), tuned for Sia:

- GF(2^8) with generator polynomial `0x11D`
- Vandermonde encoding matrix; produces the same parity bytes as klauspost's
  default `New()` and as the existing Sia client SDKs
- Optional Cauchy construction via `ReedSolomon::new_cauchy`
- AVX2 + GFNI on x86_64, NEON on aarch64, scalar elsewhere; picked at runtime
- Builds for `wasm32-unknown-unknown`

## Usage

```rust
use sia_reed_solomon::ReedSolomon;

let rs = ReedSolomon::new(10, 20)?; // 10 data + 20 parity

let mut shards: Vec<Vec<u8>> = /* 10 random shards + 20 zero-filled */;
rs.encode(&mut shards)?;
assert!(rs.verify(&shards)?);

// Drop arbitrary shards (up to 20) and reconstruct.
let mut opt: Vec<Option<Vec<u8>>> = shards.iter().cloned().map(Some).collect();
opt[0] = None;
opt[15] = None;
rs.reconstruct(&mut opt)?;
```

## Features

| Feature    | Default | Effect                                                          |
|------------|---------|-----------------------------------------------------------------|
| `parallel` | yes     | Rayon parallelism. Automatically disabled on `wasm32`.          |
| `simd`     | yes     | AVX2 / GFNI / NEON / SIMD128            |

### WASM

The `simd128` backend needs `-C target-feature=+simd128` (via `RUSTFLAGS` or `.cargo/config.toml`). 
Without it the build falls through to scalar.

## Testing

CI runs both daily and on every PR. The matrix covers all supported SIMD backends:

| Job             | Runner                  | Path under test                          |
|-----------------|-------------------------|------------------------------------------|
| `Test SIMD (AVX2)` | c5 (Skylake-SP / Zen 2-3) | AVX2 (CPUID asserts `avx2`, `!gfni`)   |
| `Test SIMD (GFNI)` | c7i (Sapphire Rapids)     | GFNI (CPUID asserts `gfni`)             |
| `Test SIMD (NEON)` | c8g (Graviton 4, arm64)   | NEON (CPUID asserts `asimd`)            |
| `Test WASM (chrome\|firefox, simd128)` | Linux | SIMD128 in headless browser  |
| `Test WASM (chrome\|firefox, scalar)`  | Linux | Scalar fallback in headless browser |
| `Test <os> (default\|no-simd\|scalar)` | Linux / macOS / Windows | Cartesian: `simd+parallel`, `parallel` only, neither |

### Testing locally

#### Native

```sh
cargo test
```

#### WASM

```sh
WASM_BINDGEN_USE_BROWSER=1 wasm-pack test --chrome --firefox --headless
```

## Benchmarks

10 data + 20 parity, 4 MiB shards, on AWS `*.4xlarge` spot runners (16 vCPU
each). SIMD is the default build; "Scalar" is `--no-default-features --features
parallel`. WASM is benched separately below.

### Throughput

| Operation                  | AVX2 (c5.4xlarge) | GFNI (c7i.4xlarge) | NEON (c7g.4xlarge) | Scalar (c7i.4xlarge) |
|----------------------------|-------------------|--------------------|--------------------|-----------------------|
| `encode`                   | 22.5 GiB/s        | 24.6 GiB/s         | 28.3 GiB/s         | 4.2 GiB/s             |
| `verify`                   | 4.4 GiB/s         | 4.4 GiB/s          | 6.1 GiB/s          | 2.4 GiB/s             |
| `reconstruct -1 data lost` | 37.6 GiB/s        | 32.9 GiB/s         | 59.1 GiB/s         | 14.9 GiB/s            |
| `reconstruct -10 data lost`| 7.2 GiB/s         | 8.2 GiB/s          | 10.7 GiB/s         | 2.4 GiB/s             |

Reconstruct throughput is per data slab (`data_shards × shard_size`), not per byte rebuilt.

### Comparisons

`reed_solomon_erasure` is built with `simd-accel`.

c5.4xlarge (AVX2):

| Operation                  | this       | klauspost (Go) | reed_solomon_erasure |
|----------------------------|------------|----------------|----------------------|
| `encode`                   | 22.5 GiB/s | 37.2 GiB/s     | 1.1 GiB/s            |
| `verify`                   | 4.4 GiB/s  | 4.1 GiB/s      | 813 MiB/s            |
| `reconstruct -1 data lost` | 37.6 GiB/s | 30.3 GiB/s     | 5.7 GiB/s            |
| `reconstruct -10 data lost`| 7.2 GiB/s  | 3.8 GiB/s      | 597 MiB/s            |

c7i.4xlarge (GFNI):

| Operation                  | this       | klauspost (Go) | reed_solomon_erasure |
|----------------------------|------------|----------------|----------------------|
| `encode`                   | 24.6 GiB/s | 54.7 GiB/s     | 1001 MiB/s           |
| `verify`                   | 4.4 GiB/s  | 4.9 GiB/s      | 833 MiB/s            |
| `reconstruct -1 data lost` | 32.9 GiB/s | 21.9 GiB/s     | 5.5 GiB/s            |
| `reconstruct -10 data lost`| 8.2 GiB/s  | 6.2 GiB/s      | 590 MiB/s            |

c7g.4xlarge (NEON, Graviton 3):

| Operation                  | this       | klauspost (Go) | reed_solomon_erasure |
|----------------------------|------------|----------------|----------------------|
| `encode`                   | 28.3 GiB/s | 48.9 GiB/s     | 1.1 GiB/s            |
| `verify`                   | 6.1 GiB/s  | 12.9 GiB/s     | 834 MiB/s            |
| `reconstruct -1 data lost` | 59.1 GiB/s | 75.9 GiB/s     | 5.9 GiB/s            |
| `reconstruct -10 data lost`| 10.7 GiB/s | 18.5 GiB/s     | 593 MiB/s            |

### WASM

`wasm32-unknown-unknown` with `-C target-feature=+simd128`, run under Node on the
c7i.4xlarge host. `reed_solomon_erasure` has no wasm SIMD path, so it runs
scalar; this crate uses its SIMD128 path.

| Operation                  | this      | reed_solomon_erasure |
|----------------------------|-----------|----------------------|
| `encode`                   | 1.6 GiB/s | 209 MiB/s            |
| `verify`                   | 1.1 GiB/s | 197 MiB/s            |
| `reconstruct -1 data lost` | 1.7 GiB/s | 843 MiB/s            |
| `reconstruct -10 data lost`| 711 MiB/s | 131 MiB/s            |

Rust benches live in [comparisons/](comparisons/) (`cargo bench -p
sia_reed_solomon_comparisons`); the wasm comparison is in
[comparisons/bench-wasm/](comparisons/bench-wasm/). The klauspost Go bench is in
[comparisons/klauspost-go/](comparisons/klauspost-go/) (`go test -bench .
-benchtime=5s ./...`).


## Why not Leopard?

GF(2^16) crates (`reed-solomon-simd`, `reed-solomon-16`,
`reed-solomon-novelpoly`) use FFT-based encoding that is faster with larger shard counts and produces different parity bytes. Matrix-based GF(2^8) provides higher performance for Sia's workflows and matches the data that already 
exists on the network. Sia's default is 30 shards and the architecture doesn't significantly benefit from higher shard counts.

On the c7i.4xlarge (GFNI) runner at Sia's default 10 + 20 / 4 MiB shards, the FFT-based
GF(2^16) codecs are 5–15× slower on encode and 16–306× slower on reconstruct
than this crate's GF(2^8) matrix.

| Operation                  | this (GF8 matrix) | reed_solomon_simd (GF16 FFT) | klauspost Leopard GF16 | klauspost Leopard GF8 |
|----------------------------|-------------------|------------------------------|-------------------------|------------------------|
| `encode`                   | **24.6 GiB/s**        | 1.7 GiB/s                    | 4.7 GiB/s               | 4.7 GiB/s              |
| `reconstruct -1 data lost` | **32.9 GiB/s**        | 110 MiB/s                   | 480 MiB/s               | 596 MiB/s              |
| `reconstruct -10 data lost`| **8.2 GiB/s**         | 109 MiB/s                   | 409 MiB/s               | 522 MiB/s              |

## License

MIT. See [LICENSE](LICENSE).
