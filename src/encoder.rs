//! Reed-Solomon encoder over GF(2^8) with Cauchy matrix construction.
//!
//! Translated from
//! [`reedsolomon.go`](https://github.com/klauspost/reedsolomon/blob/master/reedsolomon.go)
//! in Klaus Post's reedsolomon library — `New` / `newReedSolomon`, `Encode`,
//! `Verify`, `Reconstruct` / `reconstruct`, and `codeSomeShards` — but
//! omitting the streaming API, the inversion-tree cache, and the AVX2/GFNI
//! assembly paths. Sia uses only the synchronous, in-memory codec.

use crate::error::{Error, Result};
use crate::galois::{mul_slice, mul_slice_xor};
use crate::matrix::Matrix;

/// Reed-Solomon encoder.
#[derive(Debug)]
pub struct ReedSolomon {
    data_shards: usize,
    parity_shards: usize,
    matrix: Matrix,
}

impl ReedSolomon {
    /// Creates a new encoder using a Vandermonde encoding matrix —
    /// wire-compatible with klauspost/reedsolomon's default `New()`.
    pub fn new(data_shards: usize, parity_shards: usize) -> Result<Self> {
        let total = check_counts(data_shards, parity_shards)?;
        Ok(Self::with_matrix(
            data_shards,
            parity_shards,
            Matrix::vandermonde_encoding(data_shards, total)?,
        ))
    }

    /// Creates a new encoder using a Cauchy encoding matrix. Same algebraic
    /// guarantees as `new`, different parity bytes — not wire-compatible
    /// with klauspost's default.
    pub fn new_cauchy(data_shards: usize, parity_shards: usize) -> Result<Self> {
        let total = check_counts(data_shards, parity_shards)?;
        Ok(Self::with_matrix(
            data_shards,
            parity_shards,
            Matrix::cauchy(data_shards, total),
        ))
    }

    fn with_matrix(data_shards: usize, parity_shards: usize, matrix: Matrix) -> Self {
        Self {
            data_shards,
            parity_shards,
            matrix,
        }
    }

    fn parity_rows(&self) -> Vec<&[u8]> {
        (self.data_shards..self.total_shards())
            .map(|r| self.matrix.row(r))
            .collect()
    }

    #[inline]
    pub fn data_shards(&self) -> usize {
        self.data_shards
    }

    #[inline]
    pub fn parity_shards(&self) -> usize {
        self.parity_shards
    }

    #[inline]
    pub fn total_shards(&self) -> usize {
        self.data_shards + self.parity_shards
    }

    /// Encodes parity shards in-place. `shards` must have length
    /// `total_shards()`; the last `parity_shards` slots are overwritten.
    pub fn encode<T>(&self, shards: &mut [T]) -> Result<()>
    where
        T: AsRef<[u8]> + AsMut<[u8]>,
    {
        let shard_size = check_shards(shards, self.total_shards())?;
        if self.parity_shards == 0 {
            return Ok(());
        }

        let (inputs, outputs) = shards.split_at_mut(self.data_shards);
        let input_slices: Vec<&[u8]> = inputs
            .iter()
            .map(|s| {
                let s = s.as_ref();
                &s[..shard_size]
            })
            .collect();
        let output_slices: Vec<&mut [u8]> = outputs
            .iter_mut()
            .map(|s| &mut s.as_mut()[..shard_size])
            .collect();

        code_some_shards(&self.parity_rows(), &input_slices, output_slices);
        Ok(())
    }

    /// Returns `true` iff the parity shards match a re-encode of the data.
    pub fn verify<T>(&self, shards: &[T]) -> Result<bool>
    where
        T: AsRef<[u8]>,
    {
        let shard_size = check_shards(shards, self.total_shards())?;
        if self.parity_shards == 0 {
            return Ok(true);
        }

        let input_slices: Vec<&[u8]> = shards[..self.data_shards]
            .iter()
            .map(|s| &s.as_ref()[..shard_size])
            .collect();
        let expected: Vec<&[u8]> = shards[self.data_shards..]
            .iter()
            .map(|s| &s.as_ref()[..shard_size])
            .collect();

        let mut scratch: Vec<Vec<u8>> = (0..self.parity_shards)
            .map(|_| vec![0u8; shard_size])
            .collect();
        {
            let scratch_refs: Vec<&mut [u8]> =
                scratch.iter_mut().map(|s| s.as_mut_slice()).collect();
            code_some_shards(&self.parity_rows(), &input_slices, scratch_refs);
        }

        Ok(scratch.iter().zip(expected).all(|(a, b)| a.as_slice() == b))
    }

    /// Reconstructs all missing (`None`) shards in place. Present shards
    /// must have matching length. Mirrors klauspost's `Reconstruct`.
    pub fn reconstruct(&self, shards: &mut [Option<Vec<u8>>]) -> Result<()> {
        self.reconstruct_inner(shards, false)
    }

    /// Like [`reconstruct`](Self::reconstruct) but leaves missing parity
    /// shards as `None`. Mirrors klauspost's `ReconstructData`.
    pub fn reconstruct_data(&self, shards: &mut [Option<Vec<u8>>]) -> Result<()> {
        self.reconstruct_inner(shards, true)
    }

    /// Mirrors klauspost's `reconstruct` minus the inversion-tree cache:
    /// the `k × k` submatrix inverse is recomputed per call.
    fn reconstruct_inner(&self, shards: &mut [Option<Vec<u8>>], data_only: bool) -> Result<()> {
        if shards.len() != self.total_shards() {
            return Err(Error::WrongShardCount {
                expected: self.total_shards(),
                actual: shards.len(),
            });
        }

        let shard_size = shards
            .iter()
            .filter_map(|s| s.as_ref())
            .map(|s| s.len())
            .next()
            .ok_or(Error::TooFewShards {
                present: 0,
                needed: self.data_shards,
            })?;
        if shard_size == 0 {
            return Err(Error::EmptyShard);
        }
        for shard in shards.iter().flatten() {
            if shard.len() != shard_size {
                return Err(Error::ShardSizeMismatch);
            }
        }

        let data_missing = (0..self.data_shards).any(|i| shards[i].is_none());
        let parity_missing = (self.data_shards..self.total_shards()).any(|i| shards[i].is_none());
        if !data_missing && (data_only || !parity_missing) {
            return Ok(());
        }

        let present_count = shards.iter().filter(|s| s.is_some()).count();
        if present_count < self.data_shards {
            return Err(Error::TooFewShards {
                present: present_count,
                needed: self.data_shards,
            });
        }

        if data_missing {
            // Pick any `data_shards` present rows of the encoding matrix to
            // form a square `k × k` block, invert it; the inverse rows are
            // the coefficient vectors that recover each data shard from the
            // present shards.
            let mut sub = Matrix::zero(self.data_shards, self.data_shards);
            let mut sub_indices = Vec::with_capacity(self.data_shards);
            for (i, shard) in shards.iter().enumerate() {
                if shard.is_some() && sub_indices.len() < self.data_shards {
                    sub_indices.push(i);
                    let row = self.matrix.row(i);
                    let r = sub_indices.len() - 1;
                    for (c, &v) in row.iter().take(self.data_shards).enumerate() {
                        sub.set(r, c, v);
                    }
                }
            }
            let sub_inv = sub.invert()?;

            let input_slices: Vec<&[u8]> = sub_indices
                .iter()
                .map(|&i| shards[i].as_ref().unwrap().as_slice())
                .collect();

            let missing_data: Vec<usize> = (0..self.data_shards)
                .filter(|&i| shards[i].is_none())
                .collect();
            let coeff_rows: Vec<&[u8]> = missing_data.iter().map(|&d| sub_inv.row(d)).collect();

            let mut outputs: Vec<Vec<u8>> = (0..missing_data.len())
                .map(|_| vec![0u8; shard_size])
                .collect();
            {
                let out_refs: Vec<&mut [u8]> =
                    outputs.iter_mut().map(|s| s.as_mut_slice()).collect();
                code_some_shards(&coeff_rows, &input_slices, out_refs);
            }
            for (&d, out) in missing_data.iter().zip(outputs) {
                shards[d] = Some(out);
            }
        }

        if data_only {
            return Ok(());
        }

        let missing_parity: Vec<usize> = (self.data_shards..self.total_shards())
            .filter(|&i| shards[i].is_none())
            .collect();
        if missing_parity.is_empty() {
            return Ok(());
        }
        let coeff_rows: Vec<&[u8]> = missing_parity.iter().map(|&p| self.matrix.row(p)).collect();

        let input_slices: Vec<&[u8]> = (0..self.data_shards)
            .map(|i| shards[i].as_ref().unwrap().as_slice())
            .collect();

        let mut outputs: Vec<Vec<u8>> = (0..missing_parity.len())
            .map(|_| vec![0u8; shard_size])
            .collect();
        {
            let out_refs: Vec<&mut [u8]> = outputs.iter_mut().map(|s| s.as_mut_slice()).collect();
            code_some_shards(&coeff_rows, &input_slices, out_refs);
        }
        for (&p, out) in missing_parity.iter().zip(outputs) {
            shards[p] = Some(out);
        }
        Ok(())
    }
}

/// Working-set block. Sized so one block × all-shards plus `MUL_TABLE` fits
/// in L2 on the consumer CPUs we target; must be a multiple of the 64-byte
/// cacheline.
const BLOCK_SIZE: usize = 32 * 1024;
const _: () = assert!(
    BLOCK_SIZE.is_multiple_of(64),
    "BLOCK_SIZE must be a multiple of 64"
);

/// `outputs[r] = ∑_c matrix_rows[r][c] · inputs[c]` over GF(2^8).
/// Cache-blocked port of klauspost's `codeSomeShards`.
fn code_some_shards(matrix_rows: &[&[u8]], inputs: &[&[u8]], outputs: Vec<&mut [u8]>) {
    if outputs.is_empty() {
        return;
    }
    let len = inputs.first().map(|s| s.len()).unwrap_or(0);
    if len == 0 {
        return;
    }

    #[cfg(feature = "parallel")]
    {
        let n_blocks = len.div_ceil(BLOCK_SIZE);
        let n_threads = rayon::current_num_threads().max(1);
        // Skip rayon when per-thread work would be too small to amortize
        // contention.
        if n_blocks * outputs.len() >= n_threads * 2 {
            if n_blocks >= n_threads {
                code_some_shards_blocked_par(matrix_rows, inputs, outputs, len);
                return;
            }
            if outputs.len() > 1 {
                code_some_shards_rows_par(matrix_rows, inputs, outputs);
                return;
            }
        }
    }

    // Sequential fallback: still block-ordered so the same cache-locality
    // win applies on single-core builds.
    code_some_shards_blocked_seq(matrix_rows, inputs, outputs, len);
}

#[cfg(feature = "parallel")]
fn code_some_shards_rows_par(matrix_rows: &[&[u8]], inputs: &[&[u8]], outputs: Vec<&mut [u8]>) {
    use rayon::prelude::*;
    outputs.into_par_iter().enumerate().for_each(|(r, out)| {
        let coeffs = core::slice::from_ref(&matrix_rows[r]);
        let mut start = 0;
        while start < out.len() {
            let end = (start + BLOCK_SIZE).min(out.len());
            let in_block: Vec<&[u8]> = inputs.iter().map(|s| &s[start..end]).collect();
            let mut out_block: [&mut [u8]; 1] = [&mut out[start..end]];
            process_block(coeffs, &in_block, &mut out_block[..]);
            start = end;
        }
    });
}

fn code_some_shards_blocked_seq(
    matrix_rows: &[&[u8]],
    inputs: &[&[u8]],
    mut outputs: Vec<&mut [u8]>,
    len: usize,
) {
    let mut start = 0;
    while start < len {
        let end = (start + BLOCK_SIZE).min(len);
        let in_block: Vec<&[u8]> = inputs.iter().map(|s| &s[start..end]).collect();
        let mut out_block: Vec<&mut [u8]> =
            outputs.iter_mut().map(|o| &mut o[start..end]).collect();
        process_block(matrix_rows, &in_block, &mut out_block);
        start = end;
    }
}

#[cfg(feature = "parallel")]
fn code_some_shards_blocked_par(
    matrix_rows: &[&[u8]],
    inputs: &[&[u8]],
    mut outputs: Vec<&mut [u8]>,
    len: usize,
) {
    use rayon::prelude::*;

    let n_blocks = len.div_ceil(BLOCK_SIZE);
    let n_outputs = outputs.len();

    // Transpose to [block_idx][output_idx] so each rayon task owns one
    // block's worth of every output shard.
    let mut chunked: Vec<Vec<&mut [u8]>> = (0..n_blocks)
        .map(|_| Vec::with_capacity(n_outputs))
        .collect();
    for out in outputs.iter_mut() {
        for (i, chunk) in out.chunks_mut(BLOCK_SIZE).enumerate() {
            chunked[i].push(chunk);
        }
    }

    chunked
        .into_par_iter()
        .enumerate()
        .for_each(|(block_idx, mut block_outs)| {
            let start = block_idx * BLOCK_SIZE;
            let end = start + block_outs[0].len();
            let in_block: Vec<&[u8]> = inputs.iter().map(|s| &s[start..end]).collect();
            process_block(matrix_rows, &in_block, &mut block_outs);
        });
}

/// Processes one cache-friendly block. Callers pre-slice so every input
/// and output here has the same length.
#[inline(always)]
fn process_block(matrix_rows: &[&[u8]], inputs: &[&[u8]], outputs: &mut [&mut [u8]]) {
    for (r, out_chunk) in outputs.iter_mut().enumerate() {
        let coeffs = &matrix_rows[r];
        let mut wrote = false;
        for (c, &in_chunk) in inputs.iter().enumerate() {
            let coeff = coeffs[c];
            if coeff == 0 {
                continue;
            }
            if !wrote {
                mul_slice(coeff, in_chunk, out_chunk);
                wrote = true;
            } else {
                mul_slice_xor(coeff, in_chunk, out_chunk);
            }
        }
        if !wrote {
            out_chunk.fill(0);
        }
    }
}

fn check_counts(data_shards: usize, parity_shards: usize) -> Result<usize> {
    if data_shards == 0 || data_shards + parity_shards > 256 {
        return Err(Error::InvalidShardCounts {
            data: data_shards,
            parity: parity_shards,
        });
    }
    Ok(data_shards + parity_shards)
}

fn check_shards<T: AsRef<[u8]>>(shards: &[T], expected: usize) -> Result<usize> {
    if shards.len() != expected {
        return Err(Error::WrongShardCount {
            expected,
            actual: shards.len(),
        });
    }
    let size = shards[0].as_ref().len();
    if size == 0 {
        return Err(Error::EmptyShard);
    }
    for s in &shards[1..] {
        if s.as_ref().len() != size {
            return Err(Error::ShardSizeMismatch);
        }
    }
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rand_shard(seed: u8, len: usize) -> Vec<u8> {
        // Deterministic, no rand dep needed.
        (0..len)
            .map(|i| seed.wrapping_mul(31).wrapping_add(i as u8))
            .collect()
    }

    /// Golden test ported verbatim from klauspost/reedsolomon's `TestOneEncode`
    /// (reedsolomon_test.go). New(5, 5) with the default Vandermonde matrix
    /// must produce these exact parity bytes; if anything changes — field
    /// construction, matrix algorithm, multiplication tables — this test
    /// will catch it.
    ///
    /// This test is also the definitive proof that our encoder is wire-
    /// compatible with klauspost-encoded data (and therefore Sia's existing
    /// network slabs, which use the same defaults via reed-solomon-erasure).
    #[test]
    fn klauspost_one_encode_golden() {
        let rs = ReedSolomon::new(5, 5).unwrap();
        let mut shards: Vec<Vec<u8>> = vec![
            vec![0, 1],
            vec![4, 5],
            vec![2, 3],
            vec![6, 7],
            vec![8, 9],
            vec![0, 0],
            vec![0, 0],
            vec![0, 0],
            vec![0, 0],
            vec![0, 0],
        ];
        rs.encode(&mut shards).unwrap();
        // Values lifted verbatim from klauspost/reedsolomon TestOneEncode.
        assert_eq!(shards[5], vec![12, 13], "parity shard 0 (#5) mismatch");
        assert_eq!(shards[6], vec![10, 11], "parity shard 1 (#6) mismatch");
        assert_eq!(shards[7], vec![14, 15], "parity shard 2 (#7) mismatch");
        assert_eq!(shards[8], vec![90, 91], "parity shard 3 (#8) mismatch");
        assert_eq!(shards[9], vec![94, 95], "parity shard 4 (#9) mismatch");
    }

    #[test]
    fn encode_then_verify() {
        let rs = ReedSolomon::new(4, 3).unwrap();
        let mut shards: Vec<Vec<u8>> = (0..7)
            .map(|i| {
                if i < 4 {
                    rand_shard(i + 1, 128)
                } else {
                    vec![0u8; 128]
                }
            })
            .collect();
        rs.encode(&mut shards).unwrap();
        assert!(rs.verify(&shards).unwrap());
    }

    #[test]
    fn verify_fails_after_corruption() {
        let rs = ReedSolomon::new(4, 3).unwrap();
        let mut shards: Vec<Vec<u8>> = (0..7)
            .map(|i| {
                if i < 4 {
                    rand_shard(i + 1, 64)
                } else {
                    vec![0u8; 64]
                }
            })
            .collect();
        rs.encode(&mut shards).unwrap();
        shards[2][5] ^= 0xFF;
        assert!(!rs.verify(&shards).unwrap());
    }

    #[test]
    fn reconstruct_one_missing_data() {
        let rs = ReedSolomon::new(4, 3).unwrap();
        let mut shards: Vec<Vec<u8>> = (0..7)
            .map(|i| {
                if i < 4 {
                    rand_shard(i + 1, 64)
                } else {
                    vec![0u8; 64]
                }
            })
            .collect();
        rs.encode(&mut shards).unwrap();
        let original = shards.clone();

        let mut opt: Vec<Option<Vec<u8>>> = shards.into_iter().map(Some).collect();
        opt[2] = None;
        rs.reconstruct(&mut opt).unwrap();
        let rebuilt: Vec<Vec<u8>> = opt.into_iter().map(|s| s.unwrap()).collect();
        assert_eq!(rebuilt, original);
    }

    #[test]
    fn reconstruct_max_failures() {
        // Recover from `parity_shards` total drops (mix of data and parity).
        let rs = ReedSolomon::new(5, 3).unwrap();
        let mut shards: Vec<Vec<u8>> = (0..8)
            .map(|i| {
                if i < 5 {
                    rand_shard(i + 1, 200)
                } else {
                    vec![0u8; 200]
                }
            })
            .collect();
        rs.encode(&mut shards).unwrap();
        let original = shards.clone();

        let mut opt: Vec<Option<Vec<u8>>> = shards.into_iter().map(Some).collect();
        opt[1] = None;
        opt[6] = None;
        opt[7] = None;
        rs.reconstruct(&mut opt).unwrap();
        let rebuilt: Vec<Vec<u8>> = opt.into_iter().map(|s| s.unwrap()).collect();
        assert_eq!(rebuilt, original);
    }

    #[test]
    fn reconstruct_too_few_shards() {
        let rs = ReedSolomon::new(4, 2).unwrap();
        let mut shards: Vec<Vec<u8>> = (0..6)
            .map(|i| {
                if i < 4 {
                    rand_shard(i + 1, 16)
                } else {
                    vec![0u8; 16]
                }
            })
            .collect();
        rs.encode(&mut shards).unwrap();
        let mut opt: Vec<Option<Vec<u8>>> = shards.into_iter().map(Some).collect();
        opt[0] = None;
        opt[1] = None;
        opt[2] = None;
        assert!(matches!(
            rs.reconstruct(&mut opt),
            Err(Error::TooFewShards { .. })
        ));
    }

    #[test]
    fn zero_parity_shards() {
        let rs = ReedSolomon::new(3, 0).unwrap();
        let mut shards: Vec<Vec<u8>> = (0..3).map(|i| rand_shard(i + 1, 8)).collect();
        rs.encode(&mut shards).unwrap();
        assert!(rs.verify(&shards).unwrap());
    }

    #[test]
    fn wrong_shard_count_rejected() {
        let rs = ReedSolomon::new(3, 2).unwrap();
        let mut shards: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; 8]).collect();
        assert!(matches!(
            rs.encode(&mut shards),
            Err(Error::WrongShardCount {
                expected: 5,
                actual: 4
            })
        ));
    }

    #[test]
    fn shard_size_mismatch_rejected() {
        let rs = ReedSolomon::new(2, 1).unwrap();
        let mut shards: Vec<Vec<u8>> = vec![vec![0u8; 8], vec![0u8; 9], vec![0u8; 8]];
        assert!(matches!(
            rs.encode(&mut shards),
            Err(Error::ShardSizeMismatch)
        ));
    }

    #[test]
    fn invalid_shard_counts_rejected() {
        assert!(matches!(
            ReedSolomon::new(0, 1),
            Err(Error::InvalidShardCounts { .. })
        ));
        assert!(matches!(
            ReedSolomon::new(200, 100),
            Err(Error::InvalidShardCounts { .. })
        ));
    }
}
