//! NEON slice multiply. Same nibble-split shuffle trick as klauspost's
//! [`galois_arm64.s`]. 64 bytes/iter (4x VTBL pairs).
//!
//! [`galois_arm64.s`]: https://github.com/klauspost/reedsolomon/blob/master/galois_arm64.s

use core::arch::aarch64::*;

use super::MUL_TABLE;
use super::simd::{NIBBLE_TABLES, slice_loop};

const VEC_BYTES: usize = 16;
const UNROLL: usize = 4;

#[inline]
pub(super) fn mul_slice(coeff: u8, input: &[u8], out: &mut [u8]) {
    let nibble = &NIBBLE_TABLES[coeff as usize];
    // SAFETY: nibble is 32 bytes; both 16-byte loads are in bounds.
    let (lo, hi, mask) = unsafe {
        (
            vld1q_u8(nibble.as_ptr()),
            vld1q_u8(nibble.as_ptr().add(VEC_BYTES)),
            vdupq_n_u8(0x0F),
        )
    };

    slice_loop::<VEC_BYTES, UNROLL>(
        input,
        out,
        // SAFETY: slice_loop guarantees VEC_BYTES are readable/writable at p_in / p_out.
        |p_in, p_out| unsafe {
            let v = vld1q_u8(p_in);
            vst1q_u8(p_out, lookup(lo, hi, mask, v));
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
    let (lo, hi, mask) = unsafe {
        (
            vld1q_u8(nibble.as_ptr()),
            vld1q_u8(nibble.as_ptr().add(VEC_BYTES)),
            vdupq_n_u8(0x0F),
        )
    };

    slice_loop::<VEC_BYTES, UNROLL>(
        input,
        out,
        // SAFETY: slice_loop guarantees VEC_BYTES are readable/writable at p_in / p_out.
        |p_in, p_out| unsafe {
            let v = vld1q_u8(p_in);
            let e = vld1q_u8(p_out);
            vst1q_u8(p_out, veorq_u8(e, lookup(lo, hi, mask, v)));
        },
        |in_t, out_t| {
            let table = &MUL_TABLE[coeff as usize];
            for (o, &x) in out_t.iter_mut().zip(in_t) {
                *o ^= table[x as usize];
            }
        },
    );
}

// vshrq_n_u8 zero-fills the upper bits, so no AND is needed for the high nibble.
#[inline(always)]
fn lookup(lo: uint8x16_t, hi: uint8x16_t, mask: uint8x16_t, x: uint8x16_t) -> uint8x16_t {
    // SAFETY: register-only NEON ops; mandatory on aarch64.
    unsafe {
        let x_lo = vandq_u8(x, mask);
        let x_hi = vshrq_n_u8::<4>(x);
        let r_lo = vqtbl1q_u8(lo, x_lo);
        let r_hi = vqtbl1q_u8(hi, x_hi);
        veorq_u8(r_lo, r_hi)
    }
}

#[cfg(test)]
mod tests {
    use super::super::scalar;
    use super::*;

    #[test]
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
