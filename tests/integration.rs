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
    let configs = [(4usize, 2usize), (8, 8), (3, 5)];
    let sizes = [1usize, 7, 63, 64, 65, 4096, 4097, 32_768, 65_537];
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
    assert!(rs.verify(&shards).unwrap());
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

// Klauspost-compatible golden at Sia's 10-of-30 / 4 MiB sector config.
// Data shards are filled by an inline xorshift64 PRNG since Rust
// and Go do not share a compatible stdlib PRNG.
#[test]
fn klauspost_10of30_4mib_golden() {
    use blake2::digest::consts::U32;
    use blake2::{Blake2b, Digest};
    use hex_literal::hex;

    type Blake2b256 = Blake2b<U32>;

    const SECTOR_SIZE: usize = 4 * 1024 * 1024;
    const DATA_SHARDS: usize = 10;
    const PARITY_SHARDS: usize = 20;

    const EXPECTED_SHARD_HASHES: [[u8; 32]; 30] = [
        // data shards
        hex!("5f9133b3f31ca9e40e029fd0b0fc31127803ba39bbc6393da17f201c2b320bc0"),
        hex!("873f9a6c0bfb4063b3125f034b0adbafec4c6a3cf4855381640612d3bdb52c52"),
        hex!("addeec9b79e16ef8b73faa44acdd8bce937baf4261e0a2960fad431378163c9a"),
        hex!("99c7af0efa1aee38039171a95550735f7ba85f2cc53b5d211177a4714261067f"),
        hex!("7c6619b96e1518270e8a6098558d92c6f599500a4c4a07c2b1c378f1c28f81d2"),
        hex!("e4a27ad70588b5fe9b1eab2c3e90b2400f9b835870314d5462af677fa0194b65"),
        hex!("28fde42094bb60c92aef3f4c1b76ef3b41407b4f32980d1487bacd3439fc1c38"),
        hex!("49a89238c935b6dbfae3081785ce008b1e6c5b17e64e87e6a977146956708e95"),
        hex!("fe4604077368a0da69257ad0f6d4a81c1d2ecb95100b320f837c190aee42197a"),
        hex!("80bed93006c4e0a4f2aca7ee2da737271d6df50b117c1ba4012ad06381b45a84"),
        // parity shards
        hex!("d0820641e4a40d01aa61812561717a45681e0d9d990daff41971e0e4bbb9596f"),
        hex!("c93ede3459a43f28a73d6b54618891d218fe2a6fff72e8a2e11ddcc8f3c03ce3"),
        hex!("240cb1f10fb2539f287af32dab1271b37896dd72ce63e9df4dc528abe65a260c"),
        hex!("85315fa52dcc04496815bc6d988a0b2caa7a872957739fd2e1aac5189e756fcf"),
        hex!("7c5c6545793751788dd8e401d46b0567cb34bc2ee31097e1ec2108c6e01511a6"),
        hex!("24bfd4acab06d4976f08219b6fb5dc872b1382f39961f23b5d09065d137f423f"),
        hex!("fd3140df262ab81f99f1f5a4ee83a2d06f2f361b538a4949b651ad2bc24e7be5"),
        hex!("46cab3709634583d2fe357d62f8a30c4797ea26696ecfb7957b3bb5168787cfc"),
        hex!("babf9e26da954f409e2fb8834fddf2c075daa8789c62c03a2cc649296b3ad0ee"),
        hex!("08cd570feba44f78705f0b3fd5fc973bcd62beb16567c700a3671a316af6a71b"),
        hex!("a56df2e4f7be6626861da81b83e812315870ff89d0854cf290a2e42ccb64358f"),
        hex!("5264c29cfd9fe9c63cdefed4ca20c790ed30c9ff2bfd9c167bf5205d797f9f00"),
        hex!("9f1c15a3a5514581eb0e20b3811b92fcf4f59cdbd986ea2677d40f65e728aa33"),
        hex!("aaaa12e1c177e5e52012068462b83e9a0ce2c6d74d089cbdf4b370186ac386ad"),
        hex!("99f837946ab86c68b451693685041b88aa66ff1330ff2d0c54c87e87cec5640b"),
        hex!("7fc2ffab8e8c85898b2d6a225b85771cd8ceeea61306710f14f07c94076e267c"),
        hex!("9ff3bfbd1f282f9ef3705715321a687cfe7f1f8d623ef153e1ebbdb9ad4493db"),
        hex!("a922d41284f8c6c8c0d764fcd0df2f5313e84abd594787e94a097ceded6dd912"),
        hex!("4b8f9c5558cd26029a120b30b8429a28f17869c283402c0dd8e8c390fb7639c7"),
        hex!("1bdc7fdb4c601c503bf12a833a12a0a41ed717db7ee1c99ce3176ba8afeb2684"),
    ];

    fn fill_shard(buf: &mut [u8], seed: u64) {
        let mut state = seed;
        for chunk in buf.chunks_exact_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            chunk.copy_from_slice(&state.to_le_bytes());
        }
    }

    let mut shards: Vec<Vec<u8>> = (0..DATA_SHARDS + PARITY_SHARDS)
        .map(|_| vec![0u8; SECTOR_SIZE])
        .collect();
    for (i, shard) in shards[..DATA_SHARDS].iter_mut().enumerate() {
        fill_shard(shard, i as u64 + 1);
    }

    let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).unwrap();
    rs.encode(&mut shards).unwrap();

    for (i, shard) in shards.iter().enumerate() {
        let got: [u8; 32] = Blake2b256::digest(shard).into();
        assert_eq!(got, EXPECTED_SHARD_HASHES[i], "shard {i} hash mismatch");
    }
    assert!(rs.verify(&shards).unwrap());

    let check_reconstruct = |dropped: &[usize], label: &str| {
        let mut opt: Vec<Option<Vec<u8>>> = shards.iter().cloned().map(Some).collect();
        for &i in dropped {
            opt[i] = None;
        }
        rs.reconstruct_data(&mut opt).unwrap();
        for i in 0..DATA_SHARDS {
            let shard = opt[i].as_ref().expect("data shard reconstructed");
            let got: [u8; 32] = Blake2b256::digest(shard).into();
            assert_eq!(got, EXPECTED_SHARD_HASHES[i], "{label}: shard {i} mismatch");
        }
    };

    for drop in 0..DATA_SHARDS {
        check_reconstruct(&[drop], &format!("drop_{drop}"));
    }
    let all_data: Vec<usize> = (0..DATA_SHARDS).collect();
    check_reconstruct(&all_data, "all_data");
    // Minimum remaining: drop 20 (all data + half of parity), leaving exactly
    // DATA_SHARDS parity shards to reconstruct from.
    let min_remaining: Vec<usize> = (0..PARITY_SHARDS).collect();
    check_reconstruct(&min_remaining, "min_remaining");
}

#[test]
fn cauchy_round_trip() {
    let data = 10usize;
    let parity = 20usize;
    let rs = ReedSolomon::new_cauchy(data, parity).unwrap();
    let mut rng = StdRng::seed_from_u64(0xca110c);
    let mut shards = make_shards(data, data + parity, 4096, &mut rng);
    rs.encode(&mut shards).unwrap();
    assert!(rs.verify(&shards).unwrap());

    let original = shards.clone();
    let mut opt: Vec<Option<Vec<u8>>> = shards.into_iter().map(Some).collect();
    for slot in &mut opt[..parity] {
        *slot = None;
    }
    rs.reconstruct(&mut opt).unwrap();
    let rebuilt: Vec<Vec<u8>> = opt.into_iter().map(|s| s.unwrap()).collect();
    assert_eq!(rebuilt, original);
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
