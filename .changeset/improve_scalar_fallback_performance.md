---
sia_reed_solomon: minor
---

# Improve scalar fallback performance

8× unrolled `mul_slice` / `mul_slice_xor`. On c7i.4xlarge,
`--no-default-features --features parallel` (16 vCPU scalar with rayon):

| Operation                 | before     | after      | Δ    |
|---------------------------|------------|------------|------|
| `encode`                  | 3.4 GiB/s  | 4.6 GiB/s  | +35% |
| `verify`                  | 2.1 GiB/s  | 2.6 GiB/s  | +24% |
| `reconstruct -1 data`     | 13.4 GiB/s | 17.4 GiB/s | +30% |
| `reconstruct -10 data`    | 2.0 GiB/s  | 2.7 GiB/s  | +35% |
