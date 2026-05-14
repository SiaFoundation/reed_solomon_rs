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

10 data + 20 parity, 4 MiB shards, `cargo bench`. SIMD column is the default
build; scalar is `--no-default-features --features parallel`.

### Apple M-series (NEON)

| Operation                  | SIMD        | scalar     | speedup |
|----------------------------|-------------|------------|---------|
| `encode`                   | 61.9 GiB/s  | 6.3 GiB/s  | 9.8x    |
| `verify`                   | 29.8 GiB/s  | 5.8 GiB/s  | 5.1x    |
| `reconstruct -1 data lost` | 110.6 GiB/s | 34.2 GiB/s | 3.2x    |
| `reconstruct -10 data lost`| 31.3 GiB/s  | 4.1 GiB/s  | 7.6x    |

### AMD EPYC 7B13 (AVX2)

| Operation                  | SIMD       | scalar    | speedup |
|----------------------------|------------|-----------|---------|
| `encode`                   | 24.4 GiB/s | 6.8 GiB/s | 3.6x    |
| `verify`                   | 4.3 GiB/s  | 3.2 GiB/s | 1.3x    |
| `reconstruct -1 data lost` | 7.9 GiB/s  | 1.8 GiB/s | 4.3x    |
| `reconstruct -10 data lost`| 4.4 GiB/s  | 3.4 GiB/s | 1.3x    |

### Comparisons

This crate against `reed_solomon_erasure` (what `sia_storage` currently uses),
`fec_rs` (another GF(2^8) Rust crate), and klauspost/reedsolomon (the Go
library this crate ports) on the same hardware.

Apple M-series (NEON):

| Operation                  | this        | klauspost (Go) | reed_solomon_erasure | fec_rs    |
|----------------------------|-------------|----------------|----------------------|-----------|
| `encode`                   | 61.9 GiB/s  | 43.8 GiB/s     | 555 MiB/s            | 5.1 GiB/s |
| `verify`                   | 29.8 GiB/s  | 30.1 GiB/s     | 543 MiB/s            | 552 MiB/s |
| `reconstruct -1 data lost` | 110.6 GiB/s | 120.0 GiB/s    | 3.5 GiB/s            | 3.5 GiB/s |
| `reconstruct -10 data lost`| 31.3 GiB/s  | 27.6 GiB/s     | 362 MiB/s            | 359 MiB/s |

AMD EPYC 7B13 (AVX2):

| Operation                  | this       | klauspost (Go) | reed_solomon_erasure | fec_rs     |
|----------------------------|------------|----------------|----------------------|------------|
| `encode`                   | 23.3 GiB/s | 29.5 GiB/s     | 393 MiB/s            | 13.4 GiB/s |
| `verify`                   | 3.7 GiB/s  | 4.7 GiB/s      | 331 MiB/s            | 1.0 GiB/s  |
| `reconstruct -1 data lost` | 7.7 GiB/s  | 29.5 GiB/s     | 2.2 GiB/s            | 7.6 GiB/s  |
| `reconstruct -10 data lost`| 5.0 GiB/s  | 5.1 GiB/s      | 226 MiB/s            | 817 MiB/s  |

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
