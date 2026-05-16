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

| Feature    | Default | Effect                          |
|------------|---------|---------------------------------|
| `parallel` | yes     | Rayon parallelism               |
| `simd`     | yes     | AVX2 / GFNI / NEON / SIMD128    |

Disable `parallel` for WASM or other single-threaded targets; `simd` stays on
and dispatches to `simd128` on `wasm32`:

```toml
sia_reed_solomon = { version = "...", default-features = false, features = ["simd"] }
```

### WASM

The wasm `simd128` path requires building with `-C target-feature=+simd128`
(set via `RUSTFLAGS` or a `.cargo/config.toml`).

## Benchmarks

10 data + 20 parity, 4 MiB shards, on AWS `*.4xlarge` spot runners (16 vCPU
each). SIMD is the default build; "no SIMD" is `--no-default-features --features
parallel`; "WASM" is `--no-default-features` (no SIMD, no parallel — what the
`wasm32` target compiles).

### Throughput

| Operation                  | AVX2 (c5.4xlarge) | GFNI (c7i.4xlarge) | NEON (c7g.4xlarge) | no SIMD (c7i.4xlarge) | WASM (c7i.4xlarge) |
|----------------------------|-------------------|--------------------|--------------------|-----------------------|--------------------|
| `encode`                   | 22.0 GiB/s        | 30.1 GiB/s         | 28.9 GiB/s         | 4.1 GiB/s             | 673 MiB/s          |
| `verify`                   | 4.2 GiB/s         | 5.0 GiB/s          | 6.4 GiB/s          | 2.3 GiB/s             | 566 MiB/s          |
| `reconstruct -1 data lost` | 35.4 GiB/s        | 39.0 GiB/s         | 64.3 GiB/s         | 14.8 GiB/s            | 3.9 GiB/s          |
| `reconstruct -10 data lost`| 7.0 GiB/s         | 9.7 GiB/s          | 10.9 GiB/s         | 2.3 GiB/s             | 407 MiB/s          |

Reconstruct throughput is per data slab (`data_shards × shard_size`), not per
byte rebuilt.

### Comparisons

`reed_solomon_erasure` is built with `simd-accel`; `fec_rs` with `parallel`.

c5.4xlarge (AVX2):

| Operation                  | this       | klauspost (Go) | reed_solomon_erasure | fec_rs    |
|----------------------------|------------|----------------|----------------------|-----------|
| `encode`                   | 22.0 GiB/s | 35.0 GiB/s     | 1.1 GiB/s            | 5.6 GiB/s |
| `verify`                   | 4.2 GiB/s  | 3.9 GiB/s      | 743 MiB/s            | 751 MiB/s |
| `reconstruct -1 data lost` | 35.4 GiB/s | 28.5 GiB/s     | 5.6 GiB/s            | 5.5 GiB/s |
| `reconstruct -10 data lost`| 7.0 GiB/s  | 3.6 GiB/s      | 545 MiB/s            | 550 MiB/s |

c7i.4xlarge (GFNI):

| Operation                  | this       | klauspost (Go) | reed_solomon_erasure | fec_rs    |
|----------------------------|------------|----------------|----------------------|-----------|
| `encode`                   | 30.1 GiB/s | 63.5 GiB/s     | 1.3 GiB/s            | 8.9 GiB/s |
| `verify`                   | 5.0 GiB/s  | 5.5 GiB/s      | 999 MiB/s            | 1.1 GiB/s |
| `reconstruct -1 data lost` | 39.0 GiB/s | 22.5 GiB/s     | 6.1 GiB/s            | 7.4 GiB/s |
| `reconstruct -10 data lost`| 9.7 GiB/s  | 6.5 GiB/s      | 882 MiB/s            | 963 MiB/s |

c7g.4xlarge (NEON, Graviton 3):

| Operation                  | this       | klauspost (Go) | reed_solomon_erasure | fec_rs    |
|----------------------------|------------|----------------|----------------------|-----------|
| `encode`                   | 28.9 GiB/s | 49.5 GiB/s     | 1.1 GiB/s            | 2.6 GiB/s |
| `verify`                   | 6.4 GiB/s  | 13.9 GiB/s     | 854 MiB/s            | 249 MiB/s |
| `reconstruct -1 data lost` | 64.3 GiB/s | 85.9 GiB/s     | 6.0 GiB/s            | 1.7 GiB/s |
| `reconstruct -10 data lost`| 10.9 GiB/s | 18.5 GiB/s     | 618 MiB/s            | 170 MiB/s |

Rust benches live in [comparisons/](comparisons/) (`cargo bench -p
sia_reed_solomon_comparisons`). The klauspost Go bench is in
[comparisons/klauspost-go/](comparisons/klauspost-go/) (`go test -bench .
-benchtime=5s ./...`).


## Why only GF(2^8)?

GF(2^16) crates (`reed-solomon-simd`, `reed-solomon-16`,
`reed-solomon-novelpoly`) use FFT-based encoding that is faster above ~100
shards but slower below it, and produce different parity bytes. Sia uses up
to 256 shards and needs to match what its client SDKs already produce.

## License

MIT. See [LICENSE](LICENSE).
