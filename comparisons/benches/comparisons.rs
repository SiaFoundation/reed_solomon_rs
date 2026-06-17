//! Cross-crate Reed-Solomon comparison benches against `reed_solomon_erasure`
//! (what `sia_storage` currently uses) and `reed_solomon_simd` (a GF(2^16)
//! Leopard/FFT codec; encode + reconstruct only — it has no verify primitive).
//!
//! Lives in its own workspace member so the comparison dev-deps don't get
//! pulled into the main crate's build. Run with:
//!     cargo bench -p sia_reed_solomon_comparisons

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use reed_solomon_erasure::galois_8::ReedSolomon as ReedSolomonErasure;
use reed_solomon_simd::{ReedSolomonDecoder, ReedSolomonEncoder};
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
        let mut enc = ReedSolomonEncoder::new(DATA_SHARDS, PARITY_SHARDS, SHARD_SIZE).unwrap();
        group.bench_function(BenchmarkId::new("reed_solomon_simd", &label), |b| {
            b.iter(|| {
                for shard in &template[..DATA_SHARDS] {
                    enc.add_original_shard(shard).unwrap();
                }
                enc.encode().unwrap().recovery_iter().count()
            });
        });
    }
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
    group.finish();
}

fn bench_reconstruct(c: &mut Criterion) {
    let mut group = c.benchmark_group("reconstruct");
    let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
    let rs_erasure = ReedSolomonErasure::new(DATA_SHARDS, PARITY_SHARDS).unwrap();

    let mut full_rs = make_shards();
    rs.encode(&mut full_rs).unwrap();
    let mut full_erasure = make_shards();
    rs_erasure.encode(&mut full_erasure).unwrap();

    // Leopard recovery shards from the same input. full_rs[..DATA] is the
    // unmodified data (encode only writes the parity tail).
    let mut leopard_enc = ReedSolomonEncoder::new(DATA_SHARDS, PARITY_SHARDS, SHARD_SIZE).unwrap();
    for s in &full_rs[..DATA_SHARDS] {
        leopard_enc.add_original_shard(s).unwrap();
    }
    let leopard_recovery: Vec<Vec<u8>> = leopard_enc
        .encode()
        .unwrap()
        .recovery_iter()
        .map(|s| s.to_vec())
        .collect();

    let slab_bytes = DATA_SHARDS as u64 * SHARD_SIZE as u64;
    group.throughput(Throughput::Bytes(slab_bytes));

    for drop_count in [1usize, 10] {
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
            // Provide the minimal set Leopard needs: the surviving originals
            // plus `drop_count` recovery shards (DATA_SHARDS total), recovering
            // the dropped data shards.
            let mut dec = ReedSolomonDecoder::new(DATA_SHARDS, PARITY_SHARDS, SHARD_SIZE).unwrap();
            group.bench_function(BenchmarkId::new("reed_solomon_simd", &label), |b| {
                b.iter(|| {
                    for i in drop_count..DATA_SHARDS {
                        dec.add_original_shard(i, &full_rs[i]).unwrap();
                    }
                    for i in 0..drop_count {
                        dec.add_recovery_shard(i, &leopard_recovery[i]).unwrap();
                    }
                    dec.decode().unwrap().restored_original_iter().count()
                });
            });
        }
    }
    group.finish();
}

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
