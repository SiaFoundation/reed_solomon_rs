//! Criterion benchmarks for the Reed-Solomon encoder.
//!
//! Run with:
//!     cargo bench
//!     cargo bench --no-default-features   # disables rayon
//!
//! Sia's storage layer uses 4 MiB sectors and a default 10-of-30 erasure code
//! (10 data shards, 20 parity, 30 total — tolerating any 20-shard loss).
//! All benches here exercise that configuration; we vary only what we're
//! measuring (encode vs verify vs reconstruct) and the number of dropped
//! shards in the reconstruct case.
//!
//! Each operation runs under three libraries side-by-side so criterion plots
//! them in the same group for direct comparison:
//!
//!   * `reed_solomon` — this crate
//!   * `reed_solomon_erasure` — what sia_storage currently uses
//!   * `fec_rs` — another GF(2^8), poly-0x1D, WASM-compatible candidate
//!     with rayon-based parallelism (built with its `parallel` feature).

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use fec_rs::ReedSolomon as FecRs;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use reed_solomon_erasure::galois_8::ReedSolomon as ReedSolomonErasure;
use sia_reed_solomon::ReedSolomon;

/// Sia default: 10 data + 20 parity = 30 total shards, 4 MiB per shard for
/// upload (encode), 256 KiB per shard for download (reconstruct chunks).
const DATA_SHARDS: usize = 10;
const PARITY_SHARDS: usize = 20;
const SHARD_SIZE: usize = 4 * 1024 * 1024;
const DOWNLOAD_CHUNK: usize = 256 * 1024;

/// Returns `total` shards of `size` bytes: first `DATA_SHARDS` are random
/// (seeded), rest zero.
fn make_shards_sized(size: usize) -> Vec<Vec<u8>> {
    let total = DATA_SHARDS + PARITY_SHARDS;
    let mut rng = StdRng::seed_from_u64(0x51A5_51A5_51A5_51A5);
    (0..total)
        .map(|i| {
            let mut s = vec![0u8; size];
            if i < DATA_SHARDS {
                rng.fill(&mut s[..]);
            }
            s
        })
        .collect()
}

fn make_shards() -> Vec<Vec<u8>> {
    make_shards_sized(SHARD_SIZE)
}

fn bench_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode");
    // Throughput is total bytes of the shard set (data + parity), matching
    // how Sia measures upload throughput at the slab level.
    let total_bytes = (DATA_SHARDS + PARITY_SHARDS) as u64 * SHARD_SIZE as u64;
    group.throughput(Throughput::Bytes(total_bytes));
    let template = make_shards();
    let label = format!(
        "{DATA_SHARDS}of{}@{}MiB",
        DATA_SHARDS + PARITY_SHARDS,
        SHARD_SIZE / 1024 / 1024
    );

    {
        let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
        group.bench_function(BenchmarkId::new("reed_solomon", &label), |b| {
            b.iter_batched(
                || template.clone(),
                |mut shards| {
                    rs.encode(&mut shards).unwrap();
                    shards
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    {
        let rs = ReedSolomonErasure::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
        group.bench_function(BenchmarkId::new("reed_solomon_erasure", &label), |b| {
            b.iter_batched(
                || template.clone(),
                |mut shards| {
                    rs.encode(&mut shards).unwrap();
                    shards
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    {
        let rs = FecRs::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
        group.bench_function(BenchmarkId::new("fec_rs", &label), |b| {
            b.iter_batched(
                || template.clone(),
                |mut shards| {
                    rs.encode(&mut shards).unwrap();
                    shards
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_verify(c: &mut Criterion) {
    let mut group = c.benchmark_group("verify");
    let total_bytes = (DATA_SHARDS + PARITY_SHARDS) as u64 * SHARD_SIZE as u64;
    group.throughput(Throughput::Bytes(total_bytes));
    // Build a valid encoded slab once and share it across both libs; they
    // produce *different* parity bytes (the two libs use different
    // generating polynomials and matrix constructions), so we need to encode
    // separately for each.
    let label = format!(
        "{DATA_SHARDS}of{}@{}MiB",
        DATA_SHARDS + PARITY_SHARDS,
        SHARD_SIZE / 1024 / 1024
    );

    {
        let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
        let mut shards = make_shards();
        rs.encode(&mut shards).unwrap();
        group.bench_function(BenchmarkId::new("reed_solomon", &label), |b| {
            b.iter(|| {
                let ok = rs.verify(&shards).unwrap();
                assert!(ok);
            });
        });
    }
    {
        let rs = ReedSolomonErasure::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
        let mut shards = make_shards();
        rs.encode(&mut shards).unwrap();
        group.bench_function(BenchmarkId::new("reed_solomon_erasure", &label), |b| {
            b.iter(|| {
                let ok = rs.verify(&shards).unwrap();
                assert!(ok);
            });
        });
    }
    {
        let rs = FecRs::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
        let mut shards = make_shards();
        rs.encode(&mut shards).unwrap();
        group.bench_function(BenchmarkId::new("fec_rs", &label), |b| {
            b.iter(|| {
                let ok = rs.verify(&shards).unwrap();
                assert!(ok);
            });
        });
    }
    group.finish();
}

fn bench_reconstruct(c: &mut Criterion) {
    let mut group = c.benchmark_group("reconstruct");
    let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
    let rs_erasure = ReedSolomonErasure::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
    let rs_fec = FecRs::new(DATA_SHARDS, PARITY_SHARDS).unwrap();

    let mut full_rs = make_shards();
    rs.encode(&mut full_rs).unwrap();
    let mut full_erasure = make_shards();
    rs_erasure.encode(&mut full_erasure).unwrap();
    let mut full_fec = make_shards();
    rs_fec.encode(&mut full_fec).unwrap();

    // Sia commonly reads from a subset of hosts and reconstructs the rest.
    // We measure two realistic failure modes:
    //
    //   * "1 data missing"  — single host slow/down on a successful download.
    //   * "10 data missing" — pathological: every data shard lost, all parity
    //                          intact. Forces matrix inversion + full re-encode.
    for drop_count in [1usize, 10] {
        // Throughput counts bytes that had to be re-derived.
        group.throughput(Throughput::Bytes((drop_count as u64) * SHARD_SIZE as u64));
        let label = format!(
            "{DATA_SHARDS}of{}@{}MiB -{drop_count}data",
            DATA_SHARDS + PARITY_SHARDS,
            SHARD_SIZE / 1024 / 1024
        );

        {
            let mut template: Vec<Option<Vec<u8>>> = full_rs.iter().cloned().map(Some).collect();
            for slot in &mut template[..drop_count] {
                *slot = None;
            }
            group.bench_function(BenchmarkId::new("reed_solomon", &label), |b| {
                b.iter_batched(
                    || template.clone(),
                    |mut shards| {
                        rs.reconstruct_data(&mut shards).unwrap();
                        shards
                    },
                    criterion::BatchSize::LargeInput,
                );
            });
        }
        {
            let mut template: Vec<Option<Vec<u8>>> =
                full_erasure.iter().cloned().map(Some).collect();
            for slot in &mut template[..drop_count] {
                *slot = None;
            }
            group.bench_function(BenchmarkId::new("reed_solomon_erasure", &label), |b| {
                b.iter_batched(
                    || template.clone(),
                    |mut shards| {
                        rs_erasure.reconstruct_data(&mut shards).unwrap();
                        shards
                    },
                    criterion::BatchSize::LargeInput,
                );
            });
        }
        {
            let mut template: Vec<Option<Vec<u8>>> = full_fec.iter().cloned().map(Some).collect();
            for slot in &mut template[..drop_count] {
                *slot = None;
            }
            group.bench_function(BenchmarkId::new("fec_rs", &label), |b| {
                b.iter_batched(
                    || template.clone(),
                    |mut shards| {
                        rs_fec.reconstruct_data(&mut shards).unwrap();
                        shards
                    },
                    criterion::BatchSize::LargeInput,
                );
            });
        }
    }
    group.finish();
}

/// Reconstruct (10 missing data shards, 10-of-30 codec) swept across shard
/// sizes around Sia's 256 KiB download chunk. Each crate runs at each size
/// so the throughput curve is comparable: this is where our dispatch picks
/// between block-major and row-major parallelism, and it's the production
/// hot path for the download/decode side.
fn bench_reconstruct_chunked(c: &mut Criterion) {
    let mut group = c.benchmark_group("reconstruct_chunked");
    let drop_count = 10usize;
    let sizes = [
        64 * 1024,
        128 * 1024,
        DOWNLOAD_CHUNK,
        512 * 1024,
        1024 * 1024,
    ];

    for shard_size in sizes {
        group.throughput(Throughput::Bytes((drop_count as u64) * shard_size as u64));
        let label = format!("{}KiB", shard_size / 1024);

        // Encode separately per crate — different matrices produce different
        // parity bytes (we and reed_solomon_erasure both use Vandermonde, but
        // fec_rs may diverge), so the reconstruct inputs must match the
        // crate's own encoded output.
        {
            let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
            let mut full = make_shards_sized(shard_size);
            rs.encode(&mut full).unwrap();
            let mut template: Vec<Option<Vec<u8>>> = full.iter().cloned().map(Some).collect();
            for slot in &mut template[..drop_count] {
                *slot = None;
            }
            group.bench_function(BenchmarkId::new("reed_solomon", &label), |b| {
                b.iter_batched(
                    || template.clone(),
                    |mut shards| {
                        rs.reconstruct_data(&mut shards).unwrap();
                        shards
                    },
                    criterion::BatchSize::LargeInput,
                );
            });
        }
        {
            let rs = ReedSolomonErasure::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
            let mut full = make_shards_sized(shard_size);
            rs.encode(&mut full).unwrap();
            let mut template: Vec<Option<Vec<u8>>> = full.iter().cloned().map(Some).collect();
            for slot in &mut template[..drop_count] {
                *slot = None;
            }
            group.bench_function(BenchmarkId::new("reed_solomon_erasure", &label), |b| {
                b.iter_batched(
                    || template.clone(),
                    |mut shards| {
                        rs.reconstruct_data(&mut shards).unwrap();
                        shards
                    },
                    criterion::BatchSize::LargeInput,
                );
            });
        }
        {
            let rs = FecRs::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
            let mut full = make_shards_sized(shard_size);
            rs.encode(&mut full).unwrap();
            let mut template: Vec<Option<Vec<u8>>> = full.iter().cloned().map(Some).collect();
            for slot in &mut template[..drop_count] {
                *slot = None;
            }
            group.bench_function(BenchmarkId::new("fec_rs", &label), |b| {
                b.iter_batched(
                    || template.clone(),
                    |mut shards| {
                        rs.reconstruct_data(&mut shards).unwrap();
                        shards
                    },
                    criterion::BatchSize::LargeInput,
                );
            });
        }
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_encode,
    bench_verify,
    bench_reconstruct,
    bench_reconstruct_chunked,
);
criterion_main!(benches);
