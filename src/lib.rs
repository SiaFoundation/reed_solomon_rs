//! Reed-Solomon erasure coding over GF(2^8). In-memory, no streaming API.
//! Wire-compatible with klauspost/reedsolomon's default `New()`.
//!
//! ```
//! use sia_reed_solomon::ReedSolomon;
//!
//! let rs = ReedSolomon::new(4, 2).unwrap();
//! let mut shards: Vec<Vec<u8>> = vec![
//!     b"first  shard....".to_vec(),
//!     b"second shard....".to_vec(),
//!     b"third  shard....".to_vec(),
//!     b"fourth shard....".to_vec(),
//!     vec![0u8; 16],
//!     vec![0u8; 16],
//! ];
//! rs.encode(&mut shards).unwrap();
//! assert!(rs.verify(&shards).unwrap());
//!
//! let mut opt: Vec<Option<Vec<u8>>> = shards.iter().cloned().map(Some).collect();
//! opt[1] = None;
//! opt[4] = None;
//! rs.reconstruct(&mut opt).unwrap();
//! assert_eq!(opt.iter().map(|s| s.clone().unwrap()).collect::<Vec<_>>(), shards);
//! ```
//!
//! The default `parallel` feature enables [rayon](https://docs.rs/rayon)
//! parallelism across blocks (encoder) or output shards (reconstruct).
//! Disable for `wasm32-unknown-unknown` and other single-threaded targets.

mod encoder;
mod error;
mod galois;
mod matrix;

pub use encoder::ReedSolomon;
pub use error::{Error, Result};
