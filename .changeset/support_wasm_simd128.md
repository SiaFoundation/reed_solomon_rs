---
sia_reed_solomon: major
---

# Add SIMD128 support to WASM32 target

New `simd128` backend on `wasm32-unknown-unknown`, using the same nibble-split as the NEON path. 
Enabled via the `simd` feature (on by default) when compiled with `-C target-feature=+simd128`.

On c7i.4xlarge, V8 (Node), 4 MiB shards, 10+20 Vandermonde:

| Operation                 | wasm scalar | wasm + simd128 | Δ    |
|---------------------------|-------------|----------------|------|
| `encode`                  | 337 MiB/s   | 1.7 GiB/s      | 5.0× |
| `verify`                  | 308 MiB/s   | 1.1 GiB/s      | 3.7× |
| `reconstruct -1 data`     | 1.1 GiB/s   | 1.7 GiB/s      | 1.6× |
| `reconstruct -10 data`    | 205 MiB/s   | 729 MiB/s      | 3.6× |
