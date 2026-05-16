//! wasm32 `simd128` slice multiply. Same nibble-split shuffle trick as the
//! NEON path, using `u8x16_swizzle` in place of `vqtbl1q_u8`.

use core::arch::wasm32::*;

use super::MUL_TABLE;
use super::simd::{NIBBLE_TABLES, slice_loop};

const VEC_BYTES: usize = 16;
const UNROLL: usize = 4;

#[inline]
pub(super) fn mul_slice(coeff: u8, input: &[u8], out: &mut [u8]) {
    let nibble = &NIBBLE_TABLES[coeff as usize];
    // SAFETY: nibble is 32 bytes; both 16-byte loads are in bounds.
    let (lo, hi) = unsafe {
        (
            v128_load(nibble.as_ptr() as *const v128),
            v128_load(nibble.as_ptr().add(VEC_BYTES) as *const v128),
        )
    };

    slice_loop::<VEC_BYTES, UNROLL>(
        input,
        out,
        // SAFETY: slice_loop guarantees VEC_BYTES at p_in / p_out.
        |p_in, p_out| unsafe {
            let v = v128_load(p_in as *const v128);
            v128_store(p_out as *mut v128, lookup(lo, hi, v));
        },
        |in_t, out_t| {
            let table = &MUL_TABLE[coeff as usize];
            for (o, &x) in out_t.iter_mut().zip(in_t) {
                *o = table[x as usize];
            }
        },
    );
}

#[inline]
pub(super) fn mul_slice_xor(coeff: u8, input: &[u8], out: &mut [u8]) {
    let nibble = &NIBBLE_TABLES[coeff as usize];
    // SAFETY: nibble is 32 bytes; both 16-byte loads are in bounds.
    let (lo, hi) = unsafe {
        (
            v128_load(nibble.as_ptr() as *const v128),
            v128_load(nibble.as_ptr().add(VEC_BYTES) as *const v128),
        )
    };

    slice_loop::<VEC_BYTES, UNROLL>(
        input,
        out,
        // SAFETY: slice_loop guarantees VEC_BYTES at p_in / p_out.
        |p_in, p_out| unsafe {
            let v = v128_load(p_in as *const v128);
            let e = v128_load(p_out as *const v128);
            v128_store(p_out as *mut v128, v128_xor(e, lookup(lo, hi, v)));
        },
        |in_t, out_t| {
            let table = &MUL_TABLE[coeff as usize];
            for (o, &x) in out_t.iter_mut().zip(in_t) {
                *o ^= table[x as usize];
            }
        },
    );
}

#[inline(always)]
fn lookup(lo: v128, hi: v128, x: v128) -> v128 {
    let mask = u8x16_splat(0x0F);
    let x_lo = v128_and(x, mask);
    let x_hi = u8x16_shr(x, 4);
    let r_lo = u8x16_swizzle(lo, x_lo);
    let r_hi = u8x16_swizzle(hi, x_hi);
    v128_xor(r_lo, r_hi)
}

#[cfg(test)]
mod tests {
    use super::super::scalar;
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn matches_scalar_exhaustive() {
        let lengths = [
            0usize, 1, 7, 15, 16, 17, 31, 32, 33, 47, 48, 63, 64, 65, 100, 127, 128, 255, 256, 1024,
        ];
        let input: Vec<u8> = (0..1024).map(|i| (i & 0xFF) as u8).collect();

        for c in 0..=255u8 {
            for &n in &lengths {
                let mut out_simd = vec![0u8; n];
                let mut out_scalar = vec![0u8; n];
                mul_slice(c, &input[..n], &mut out_simd);
                scalar::mul_slice(c, &input[..n], &mut out_scalar);
                assert_eq!(out_simd, out_scalar, "mul_slice c={c} n={n}");

                let seed: Vec<u8> = (0..n).map(|i| (i as u8) ^ 0x5A).collect();
                let mut xor_simd = seed.clone();
                let mut xor_scalar = seed.clone();
                mul_slice_xor(c, &input[..n], &mut xor_simd);
                scalar::mul_slice_xor(c, &input[..n], &mut xor_scalar);
                assert_eq!(xor_simd, xor_scalar, "mul_slice_xor c={c} n={n}");
            }
        }
    }
}
