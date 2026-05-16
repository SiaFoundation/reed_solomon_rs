//! Scalar fallback. Ported from klauspost's `galMulSlice` / `galMulSliceXor`:
//! index one 256-byte row of MUL_TABLE per input byte.

use super::MUL_TABLE;

const UNROLL: usize = 8;

#[inline]
#[allow(dead_code)]
pub(super) fn mul_slice(coeff: u8, input: &[u8], out: &mut [u8]) {
    debug_assert_eq!(input.len(), out.len());
    let table: &[u8; 256] = &MUL_TABLE[coeff as usize];
    let mut in_chunks = input.chunks_exact(UNROLL);
    let mut out_chunks = out.chunks_exact_mut(UNROLL);
    for (oc, ic) in (&mut out_chunks).zip(&mut in_chunks) {
        oc[0] = table[ic[0] as usize];
        oc[1] = table[ic[1] as usize];
        oc[2] = table[ic[2] as usize];
        oc[3] = table[ic[3] as usize];
        oc[4] = table[ic[4] as usize];
        oc[5] = table[ic[5] as usize];
        oc[6] = table[ic[6] as usize];
        oc[7] = table[ic[7] as usize];
    }
    for (o, &x) in out_chunks
        .into_remainder()
        .iter_mut()
        .zip(in_chunks.remainder())
    {
        *o = table[x as usize];
    }
}

#[inline]
#[allow(dead_code)]
pub(super) fn mul_slice_xor(coeff: u8, input: &[u8], out: &mut [u8]) {
    debug_assert_eq!(input.len(), out.len());
    let table: &[u8; 256] = &MUL_TABLE[coeff as usize];
    let mut in_chunks = input.chunks_exact(UNROLL);
    let mut out_chunks = out.chunks_exact_mut(UNROLL);
    for (oc, ic) in (&mut out_chunks).zip(&mut in_chunks) {
        oc[0] ^= table[ic[0] as usize];
        oc[1] ^= table[ic[1] as usize];
        oc[2] ^= table[ic[2] as usize];
        oc[3] ^= table[ic[3] as usize];
        oc[4] ^= table[ic[4] as usize];
        oc[5] ^= table[ic[5] as usize];
        oc[6] ^= table[ic[6] as usize];
        oc[7] ^= table[ic[7] as usize];
    }
    for (o, &x) in out_chunks
        .into_remainder()
        .iter_mut()
        .zip(in_chunks.remainder())
    {
        *o ^= table[x as usize];
    }
}

#[cfg(test)]
mod tests {
    use super::super::mul;
    use super::*;

    // Pins the XOR-into-out semantic. mul_slice itself is covered by
    // `mul_table_matches_mul` in galois.rs.
    #[test]
    fn mul_slice_xor_accumulates() {
        let input: Vec<u8> = (0..200u32).map(|x| (x as u8).wrapping_mul(7)).collect();
        let mut out = vec![0xABu8; input.len()];
        let coeff = 17u8;
        let expected: Vec<u8> = input.iter().map(|&x| 0xAB ^ mul(coeff, x)).collect();
        mul_slice_xor(coeff, &input, &mut out);
        assert_eq!(out, expected);
    }
}
