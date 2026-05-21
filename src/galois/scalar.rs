//! Scalar fallback. Ported from klauspost's `galMulSlice` / `galMulSliceXor`:
//! index one 256-byte row of MUL_TABLE per input byte.

use super::MUL_TABLE;

#[inline]
#[allow(dead_code)]
pub(super) fn mul_slice(coeff: u8, input: &[u8], out: &mut [u8]) {
    debug_assert_eq!(input.len(), out.len());
    let table: &[u8; 256] = &MUL_TABLE[coeff as usize];
    let (in_chunks, in_rem) = input.as_chunks::<8>();
    let (out_chunks, out_rem) = out.as_chunks_mut::<8>();
    for (oc, ic) in out_chunks.iter_mut().zip(in_chunks) {
        oc[0] = table[ic[0] as usize];
        oc[1] = table[ic[1] as usize];
        oc[2] = table[ic[2] as usize];
        oc[3] = table[ic[3] as usize];
        oc[4] = table[ic[4] as usize];
        oc[5] = table[ic[5] as usize];
        oc[6] = table[ic[6] as usize];
        oc[7] = table[ic[7] as usize];
    }
    for (o, &x) in out_rem.iter_mut().zip(in_rem) {
        *o = table[x as usize];
    }
}

#[inline]
#[allow(dead_code)]
pub(super) fn mul_slice_xor(coeff: u8, input: &[u8], out: &mut [u8]) {
    debug_assert_eq!(input.len(), out.len());
    let table: &[u8; 256] = &MUL_TABLE[coeff as usize];
    let (in_chunks, in_rem) = input.as_chunks::<8>();
    let (out_chunks, out_rem) = out.as_chunks_mut::<8>();
    for (oc, ic) in out_chunks.iter_mut().zip(in_chunks) {
        oc[0] ^= table[ic[0] as usize];
        oc[1] ^= table[ic[1] as usize];
        oc[2] ^= table[ic[2] as usize];
        oc[3] ^= table[ic[3] as usize];
        oc[4] ^= table[ic[4] as usize];
        oc[5] ^= table[ic[5] as usize];
        oc[6] ^= table[ic[6] as usize];
        oc[7] ^= table[ic[7] as usize];
    }
    for (o, &x) in out_rem.iter_mut().zip(in_rem) {
        *o ^= table[x as usize];
    }
}

#[cfg(test)]
mod tests {
    use super::super::mul;
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

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
