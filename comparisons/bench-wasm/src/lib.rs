//! Wasm-bindgen comparison harness pitting `sia_reed_solomon` against
//! `reed_solomon_erasure` and `fec_rs` on the same shard layout. The
//! `node run.mjs` driver times all three. `reconstruct` uses the data-only
//! path (`reconstruct_data`) on each, matching the native cross-crate benches
//! in `comparisons/`.

use fec_rs::ReedSolomon as FecRs;
use reed_solomon_erasure::galois_8::ReedSolomon as ErasureRs;
use sia_reed_solomon::ReedSolomon as SiaRs;
use wasm_bindgen::prelude::*;

const DATA: usize = 10;
const PARITY: usize = 20;

fn make_shards(shard_size: usize) -> Vec<Vec<u8>> {
    (0..DATA + PARITY)
        .map(|i| {
            let mut s = vec![0u8; shard_size];
            if i < DATA {
                for (j, b) in s.iter_mut().enumerate() {
                    *b = ((i * 31 + j * 17) & 0xff) as u8;
                }
            }
            s
        })
        .collect()
}

#[wasm_bindgen]
pub struct SiaHarness {
    rs: SiaRs,
    shards: Vec<Vec<u8>>,
}

#[wasm_bindgen]
impl SiaHarness {
    #[wasm_bindgen(constructor)]
    pub fn new(shard_size: usize) -> Self {
        let rs = SiaRs::new(DATA, PARITY).unwrap();
        let mut shards = make_shards(shard_size);
        rs.encode(&mut shards).unwrap();
        Self { rs, shards }
    }

    pub fn encode_iter(&mut self) {
        for s in &mut self.shards[DATA..] {
            s.fill(0);
        }
        self.rs.encode(&mut self.shards).unwrap();
    }

    pub fn verify_iter(&self) -> bool {
        self.rs.verify(&self.shards).unwrap()
    }

    pub fn reconstruct_iter(&mut self, drop_count: usize) {
        let mut opt: Vec<Option<Vec<u8>>> = self.shards.iter().cloned().map(Some).collect();
        for slot in &mut opt[..drop_count] {
            *slot = None;
        }
        self.rs.reconstruct_data(&mut opt).unwrap();
    }
}

#[wasm_bindgen]
pub struct ErasureHarness {
    rs: ErasureRs,
    shards: Vec<Vec<u8>>,
}

#[wasm_bindgen]
impl ErasureHarness {
    #[wasm_bindgen(constructor)]
    pub fn new(shard_size: usize) -> Self {
        let rs = ErasureRs::new(DATA, PARITY).unwrap();
        let mut shards = make_shards(shard_size);
        rs.encode(&mut shards).unwrap();
        Self { rs, shards }
    }

    pub fn encode_iter(&mut self) {
        for s in &mut self.shards[DATA..] {
            s.fill(0);
        }
        self.rs.encode(&mut self.shards).unwrap();
    }

    pub fn verify_iter(&self) -> bool {
        self.rs.verify(&self.shards).unwrap()
    }

    pub fn reconstruct_iter(&mut self, drop_count: usize) {
        let mut opt: Vec<Option<Vec<u8>>> = self.shards.iter().cloned().map(Some).collect();
        for slot in &mut opt[..drop_count] {
            *slot = None;
        }
        self.rs.reconstruct_data(&mut opt).unwrap();
    }
}

#[wasm_bindgen]
pub struct FecHarness {
    rs: FecRs,
    shards: Vec<Vec<u8>>,
}

#[wasm_bindgen]
impl FecHarness {
    #[wasm_bindgen(constructor)]
    pub fn new(shard_size: usize) -> Self {
        let rs = FecRs::new(DATA, PARITY).unwrap();
        let mut shards = make_shards(shard_size);
        rs.encode(&mut shards).unwrap();
        Self { rs, shards }
    }

    pub fn encode_iter(&mut self) {
        for s in &mut self.shards[DATA..] {
            s.fill(0);
        }
        self.rs.encode(&mut self.shards).unwrap();
    }

    pub fn verify_iter(&self) -> bool {
        self.rs.verify(&self.shards).unwrap()
    }

    pub fn reconstruct_iter(&mut self, drop_count: usize) {
        let mut opt: Vec<Option<Vec<u8>>> = self.shards.iter().cloned().map(Some).collect();
        for slot in &mut opt[..drop_count] {
            *slot = None;
        }
        self.rs.reconstruct_data(&mut opt).unwrap();
    }
}
