//! Scalar fallback. Ported from klauspost's `galMulSlice` / `galMulSliceXor`:
//! index one 256-byte row of MUL_TABLE per input byte.

use super::MUL_TABLE;

#[inline]
#[allow(dead_code)]
pub(super) fn mul_slice(coeff: u8, input: &[u8], out: &mut [u8]) {
    debug_assert_eq!(input.len(), out.len());
    let table = &MUL_TABLE[coeff as usize];
    for (o, &x) in out.iter_mut().zip(input.iter()) {
        *o = table[x as usize];
    }
}

#[inline]
#[allow(dead_code)]
pub(super) fn mul_slice_xor(coeff: u8, input: &[u8], out: &mut [u8]) {
    debug_assert_eq!(input.len(), out.len());
    let table = &MUL_TABLE[coeff as usize];
    for (o, &x) in out.iter_mut().zip(input.iter()) {
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
