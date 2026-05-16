use sia_reed_solomon::ReedSolomon;
use wasm_bindgen::prelude::*;

const DATA: usize = 10;
const PARITY: usize = 20;

#[wasm_bindgen]
pub struct Harness {
    rs: ReedSolomon,
    shards: Vec<Vec<u8>>,
}

#[wasm_bindgen]
impl Harness {
    #[wasm_bindgen(constructor)]
    pub fn new(shard_size: usize) -> Self {
        let rs = ReedSolomon::new(DATA, PARITY).unwrap();
        let mut shards: Vec<Vec<u8>> = (0..DATA + PARITY)
            .map(|i| {
                let mut s = vec![0u8; shard_size];
                if i < DATA {
                    for (j, b) in s.iter_mut().enumerate() {
                        *b = ((i * 31 + j * 17) & 0xff) as u8;
                    }
                }
                s
            })
            .collect();
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
        self.rs.reconstruct(&mut opt).unwrap();
    }
}
