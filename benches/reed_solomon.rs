//! Criterion benchmarks for the Reed-Solomon encoder.
//!
//! Run with:
//!     cargo bench
//!     cargo bench --no-default-features --features parallel   # scalar
//!
//! Sia uses 4 MiB sectors and a default 10-of-30 erasure code (10 data, 20
//! parity, tolerates any 20-shard loss); we vary only the operation under
//! test and the number of dropped shards in the reconstruct case.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use sia_reed_solomon::ReedSolomon;

const DATA_SHARDS: usize = 10;
const PARITY_SHARDS: usize = 20;
const SHARD_SIZE: usize = 4 * 1024 * 1024;
const DOWNLOAD_CHUNK: usize = 256 * 1024;

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
    let total_bytes = (DATA_SHARDS + PARITY_SHARDS) as u64 * SHARD_SIZE as u64;
    group.throughput(Throughput::Bytes(total_bytes));
    let template = make_shards();
    let label = format!(
        "{DATA_SHARDS}of{}@{}MiB",
        DATA_SHARDS + PARITY_SHARDS,
        SHARD_SIZE / 1024 / 1024
    );

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
    group.finish();
}

fn bench_verify(c: &mut Criterion) {
    let mut group = c.benchmark_group("verify");
    let total_bytes = (DATA_SHARDS + PARITY_SHARDS) as u64 * SHARD_SIZE as u64;
    group.throughput(Throughput::Bytes(total_bytes));
    let label = format!(
        "{DATA_SHARDS}of{}@{}MiB",
        DATA_SHARDS + PARITY_SHARDS,
        SHARD_SIZE / 1024 / 1024
    );

    let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
    let mut shards = make_shards();
    rs.encode(&mut shards).unwrap();
    group.bench_function(BenchmarkId::new("reed_solomon", &label), |b| {
        b.iter(|| {
            let ok = rs.verify(&shards).unwrap();
            assert!(ok);
        });
    });
    group.finish();
}

fn bench_reconstruct(c: &mut Criterion) {
    let mut group = c.benchmark_group("reconstruct");
    let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
    let mut full = make_shards();
    rs.encode(&mut full).unwrap();

    // Throughput uses the slab's data-payload size, not bytes-rebuilt:
    // the algorithm processes all k surviving shards regardless of drop
    // count, so this makes -1 and -10 directly comparable.
    let slab_bytes = DATA_SHARDS as u64 * SHARD_SIZE as u64;
    group.throughput(Throughput::Bytes(slab_bytes));

    // -1 covers the single-host-lost common case; -10 forces a full rebuild
    // from parity (every data shard lost).
    for drop_count in [1usize, 10] {
        let label = format!(
            "{DATA_SHARDS}of{}@{}MiB -{drop_count}data",
            DATA_SHARDS + PARITY_SHARDS,
            SHARD_SIZE / 1024 / 1024
        );

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
    group.finish();
}

/// Reconstruct (10 missing data shards) swept across shard sizes around
/// Sia's 256 KiB download chunk; this is where the dispatch picks between
/// block-major and row-major parallelism.
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
        group.throughput(Throughput::Bytes(DATA_SHARDS as u64 * shard_size as u64));
        let label = format!("{}KiB", shard_size / 1024);

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
