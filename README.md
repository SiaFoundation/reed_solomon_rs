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
| `simd`     | yes     | AVX2 / GFNI / NEON              |

Disable both for WASM or other single-threaded targets:

```toml
sia_reed_solomon = { version = "...", default-features = false }
```

## Benchmarks

10 data + 20 parity, 4 MiB shards, on AWS `*.4xlarge` spot runners (16 vCPU
each). SIMD is the default build; "no SIMD" is `--no-default-features --features
parallel`; "WASM" is `--no-default-features` (no SIMD, no parallel — what the
`wasm32` target compiles).

### Throughput

| Operation                  | AVX2 (c5.4xlarge) | GFNI (c7i.4xlarge) | NEON (c7g.4xlarge) | no SIMD (c7i.4xlarge) | WASM (c7i.4xlarge) |
|----------------------------|-------------------|--------------------|--------------------|-----------------------|--------------------|
| `encode`                   | 20.1 GiB/s        | 26.5 GiB/s         | 28.2 GiB/s         | 3.4 GiB/s             | 471 MiB/s          |
| `verify`                   | 4.0 GiB/s         | 4.7 GiB/s          | 6.0 GiB/s          | 2.1 GiB/s             | 409 MiB/s          |
| `reconstruct -1 data lost` | 34.3 GiB/s        | 33.5 GiB/s         | 60.1 GiB/s         | 13.4 GiB/s            | 2.6 GiB/s          |
| `reconstruct -10 data lost`| 6.6 GiB/s         | 8.3 GiB/s          | 10.6 GiB/s         | 2.0 GiB/s             | 290 MiB/s          |

Reconstruct throughput is per data slab (`data_shards × shard_size`), not per
byte rebuilt.

### Comparisons

c5.4xlarge (AVX2):

| Operation                  | this       | klauspost (Go) | reed_solomon_erasure | fec_rs    |
|----------------------------|------------|----------------|----------------------|-----------|
| `encode`                   | 20.1 GiB/s | 34.3 GiB/s     | 356 MiB/s            | 5.6 GiB/s |
| `verify`                   | 4.0 GiB/s  | 3.7 GiB/s      | 304 MiB/s            | 715 MiB/s |
| `reconstruct -1 data lost` | 34.3 GiB/s | 23.4 GiB/s     | 2.1 GiB/s            | 5.2 GiB/s |
| `reconstruct -10 data lost`| 6.6 GiB/s  | 3.2 GiB/s      | 212 MiB/s            | 530 MiB/s |

c7i.4xlarge (GFNI):

| Operation                  | this       | klauspost (Go) | reed_solomon_erasure | fec_rs    |
|----------------------------|------------|----------------|----------------------|-----------|
| `encode`                   | 26.5 GiB/s | 57.8 GiB/s     | 566 MiB/s            | 8.1 GiB/s |
| `verify`                   | 4.7 GiB/s  | 5.2 GiB/s      | 473 MiB/s            | 990 MiB/s |
| `reconstruct -1 data lost` | 33.5 GiB/s | 22.2 GiB/s     | 3.3 GiB/s            | 6.7 GiB/s |
| `reconstruct -10 data lost`| 8.3 GiB/s  | 6.4 GiB/s      | 341 MiB/s            | 846 MiB/s |

c7g.4xlarge (NEON, Graviton 3):

| Operation                  | this       | klauspost (Go) | reed_solomon_erasure | fec_rs    |
|----------------------------|------------|----------------|----------------------|-----------|
| `encode`                   | 28.2 GiB/s | 49.3 GiB/s     | 267 MiB/s            | 2.6 GiB/s |
| `verify`                   | 6.0 GiB/s  | 13.5 GiB/s     | 248 MiB/s            | 248 MiB/s |
| `reconstruct -1 data lost` | 60.1 GiB/s | 75.6 GiB/s     | 1.7 GiB/s            | 1.7 GiB/s |
| `reconstruct -10 data lost`| 10.6 GiB/s | 18.1 GiB/s     | 169 MiB/s            | 169 MiB/s |

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
