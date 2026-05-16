use rand::rngs::StdRng;
use rand::{Rng, RngExt, SeedableRng};
use sia_reed_solomon::ReedSolomon;

// Shadows the built-in `#[test]` on wasm32 so the tests below also run under
// wasm-bindgen-test-runner.
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test as test;
#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

/// Klauspost-generated golden bytes at Sia's 10-of-30 / 256 B config.
/// Layout: `[u8 data, u8 parity, u32_le size, then (data+parity)*size bytes]`.
const GOLDEN_10_OF_30: &[u8] = include_bytes!("data/golden_10of30_256.bin");

fn parse_golden(bytes: &[u8]) -> (usize, usize, usize, Vec<Vec<u8>>) {
    let data = bytes[0] as usize;
    let parity = bytes[1] as usize;
    let size = u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]) as usize;
    let total = data + parity;
    let mut shards = Vec::with_capacity(total);
    let mut off = 6;
    for _ in 0..total {
        shards.push(bytes[off..off + size].to_vec());
        off += size;
    }
    assert_eq!(
        off,
        bytes.len(),
        "golden blob has unexpected trailing bytes"
    );
    (data, parity, size, shards)
}

fn make_shards(data: usize, total: usize, shard_size: usize, rng: &mut StdRng) -> Vec<Vec<u8>> {
    (0..total)
        .map(|i| {
            let mut s = vec![0u8; shard_size];
            if i < data {
                rng.fill_bytes(&mut s);
            }
            s
        })
        .collect()
}

#[test]
fn round_trip_varies_shard_sizes() {
    // Sizes that aren't multiples of u64 / cacheline, to hit the tail path.
    let configs = [(4usize, 2usize), (8, 8), (3, 5)];
    let sizes = [1usize, 7, 63, 64, 65, 4096, 4097];
    let mut rng = StdRng::seed_from_u64(0xc0ffee);
    for (data, parity) in configs {
        for size in sizes {
            let rs = ReedSolomon::new(data, parity).unwrap();
            let mut shards = make_shards(data, data + parity, size, &mut rng);
            rs.encode(&mut shards).unwrap();
            let original = shards.clone();
            let mut opt: Vec<Option<Vec<u8>>> = shards.into_iter().map(Some).collect();
            let to_drop = parity.min(data);
            for slot in &mut opt[..to_drop] {
                *slot = None;
            }
            rs.reconstruct(&mut opt).unwrap();
            let rebuilt: Vec<Vec<u8>> = opt.into_iter().map(|s| s.unwrap()).collect();
            assert_eq!(
                rebuilt, original,
                "round-trip failed at data={data}, parity={parity}, size={size}"
            );
        }
    }
}

#[test]
fn reconstruct_data_leaves_parity_holes_alone() {
    let rs = ReedSolomon::new(6, 4).unwrap();
    let mut rng = StdRng::seed_from_u64(1);
    let mut shards = make_shards(6, 10, 256, &mut rng);
    rs.encode(&mut shards).unwrap();

    let mut opt: Vec<Option<Vec<u8>>> = shards.iter().cloned().map(Some).collect();
    opt[1] = None;
    opt[7] = None;
    opt[8] = None;
    rs.reconstruct_data(&mut opt).unwrap();

    assert_eq!(opt[1].as_ref().unwrap(), &shards[1]);
    assert!(opt[7].is_none(), "parity shard should not be rebuilt");
    assert!(opt[8].is_none(), "parity shard should not be rebuilt");
}

#[test]
fn klauspost_10of30_golden_encode() {
    let (data, parity, size, golden) = parse_golden(GOLDEN_10_OF_30);
    assert_eq!((data, parity, size), (10, 20, 256));

    let rs = ReedSolomon::new(data, parity).unwrap();
    let mut shards: Vec<Vec<u8>> = (0..(data + parity))
        .map(|i| {
            if i < data {
                golden[i].clone()
            } else {
                vec![0u8; size]
            }
        })
        .collect();
    rs.encode(&mut shards).unwrap();
    for i in 0..(data + parity) {
        assert_eq!(
            shards[i], golden[i],
            "shard {i} differs from klauspost golden output"
        );
    }
}

#[test]
fn klauspost_10of30_golden_drop_each_shard() {
    let (data, parity, _, golden) = parse_golden(GOLDEN_10_OF_30);
    let rs = ReedSolomon::new(data, parity).unwrap();
    for drop_idx in 0..(data + parity) {
        let mut opt: Vec<Option<Vec<u8>>> = golden.iter().cloned().map(Some).collect();
        opt[drop_idx] = None;
        rs.reconstruct(&mut opt).unwrap();
        let rebuilt: Vec<Vec<u8>> = opt.into_iter().map(|s| s.unwrap()).collect();
        for i in 0..(data + parity) {
            assert_eq!(
                rebuilt[i], golden[i],
                "after dropping shard {drop_idx}, shard {i} did not match \
                 klauspost golden"
            );
        }
    }
}

#[test]
fn klauspost_10of30_golden_drop_max() {
    let (data, parity, _, golden) = parse_golden(GOLDEN_10_OF_30);
    let total = data + parity;
    let rs = ReedSolomon::new(data, parity).unwrap();

    for start in 0..=(total - parity) {
        let mut opt: Vec<Option<Vec<u8>>> = golden.iter().cloned().map(Some).collect();
        for slot in &mut opt[start..start + parity] {
            *slot = None;
        }
        rs.reconstruct(&mut opt).unwrap();
        let rebuilt: Vec<Vec<u8>> = opt.into_iter().map(|s| s.unwrap()).collect();
        assert_eq!(
            rebuilt, golden,
            "max-drop window starting at {start} failed to round-trip"
        );
    }

    // Edge case: drop all data shards, reconstruct from parity alone.
    let mut opt: Vec<Option<Vec<u8>>> = golden.iter().cloned().map(Some).collect();
    for slot in &mut opt[..data] {
        *slot = None;
    }
    rs.reconstruct(&mut opt).unwrap();
    let rebuilt: Vec<Vec<u8>> = opt.into_iter().map(|s| s.unwrap()).collect();
    assert_eq!(rebuilt, golden, "dropping all data shards failed");
}

// Complements the contiguous-window coverage in `klauspost_10of30_*`.
#[test]
fn reconstruct_arbitrary_drop_patterns() {
    let data = 5usize;
    let parity = 3usize;
    let total = data + parity;
    let rs = ReedSolomon::new(data, parity).unwrap();
    let mut rng = StdRng::seed_from_u64(0xface_d00d);
    let mut shards = make_shards(data, total, 128, &mut rng);
    rs.encode(&mut shards).unwrap();
    let pristine = shards.clone();

    for _ in 0..100 {
        // Pick a random subset of `parity` positions to drop.
        let mut indices: Vec<usize> = (0..total).collect();
        for i in (1..indices.len()).rev() {
            indices.swap(i, rng.random_range(0..=i));
        }
        let drop: Vec<usize> = indices.into_iter().take(parity).collect();

        let mut opt: Vec<Option<Vec<u8>>> = pristine.iter().cloned().map(Some).collect();
        for &i in &drop {
            opt[i] = None;
        }
        rs.reconstruct(&mut opt).unwrap();
        let rebuilt: Vec<Vec<u8>> = opt.into_iter().map(|s| s.unwrap()).collect();
        assert_eq!(rebuilt, pristine, "drop pattern {drop:?}");
    }
}

#[test]
fn boundary_shard_counts() {
    let rs = ReedSolomon::new(128, 128).unwrap();
    let mut rng = StdRng::seed_from_u64(42);
    let mut shards = make_shards(128, 256, 32, &mut rng);
    rs.encode(&mut shards).unwrap();
    assert!(rs.verify(&shards).unwrap());
    assert!(ReedSolomon::new(129, 128).is_err());
}
