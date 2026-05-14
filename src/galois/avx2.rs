//! AVX2 slice multiply via VPSHUFB. See klauspost's `_avx2` paths in
//! [`galois_gen_amd64.s`].
//!
//! [`galois_gen_amd64.s`]: https://github.com/klauspost/reedsolomon/blob/master/galois_gen_amd64.s

use core::arch::x86_64::*;

use super::MUL_TABLE;
use super::simd::{NIBBLE_TABLES, slice_loop};

const VEC_BYTES: usize = 32;
const UNROLL: usize = 2;

#[target_feature(enable = "avx2")]
pub(super) fn mul_slice(coeff: u8, input: &[u8], out: &mut [u8]) {
    let nibble = &NIBBLE_TABLES[coeff as usize];
    let (lo, hi, mask) = load_tables(nibble);

    slice_loop::<VEC_BYTES, UNROLL>(
        input,
        out,
        // SAFETY: slice_loop guarantees VEC_BYTES at p_in / p_out.
        |p_in, p_out| unsafe {
            let v = _mm256_loadu_si256(p_in.cast());
            _mm256_storeu_si256(p_out.cast(), lookup(lo, hi, mask, v));
        },
        |in_t, out_t| {
            let table = &MUL_TABLE[coeff as usize];
            for (o, &x) in out_t.iter_mut().zip(in_t) {
                *o = table[x as usize];
            }
        },
    );
}

#[target_feature(enable = "avx2")]
pub(super) fn mul_slice_xor(coeff: u8, input: &[u8], out: &mut [u8]) {
    let nibble = &NIBBLE_TABLES[coeff as usize];
    let (lo, hi, mask) = load_tables(nibble);

    slice_loop::<VEC_BYTES, UNROLL>(
        input,
        out,
        // SAFETY: slice_loop guarantees VEC_BYTES at p_in / p_out.
        |p_in, p_out| unsafe {
            let p_in = p_in.cast::<__m256i>();
            let p_out = p_out.cast::<__m256i>();
            let v = _mm256_loadu_si256(p_in);
            let e = _mm256_loadu_si256(p_out);
            _mm256_storeu_si256(p_out, _mm256_xor_si256(e, lookup(lo, hi, mask, v)));
        },
        |in_t, out_t| {
            let table = &MUL_TABLE[coeff as usize];
            for (o, &x) in out_t.iter_mut().zip(in_t) {
                *o ^= table[x as usize];
            }
        },
    );
}

// lo/hi nibble tables broadcast across both 128-bit ymm lanes, plus a 0xF mask.
#[target_feature(enable = "avx2")]
#[inline]
fn load_tables(table: &[u8; 32]) -> (__m256i, __m256i, __m256i) {
    // SAFETY: table is 32 bytes; both 16-byte loads are in bounds.
    unsafe {
        let lo128 = _mm_loadu_si128(table.as_ptr().cast::<__m128i>());
        let hi128 = _mm_loadu_si128(table.as_ptr().add(16).cast::<__m128i>());
        let lo = _mm256_broadcastsi128_si256(lo128);
        let hi = _mm256_broadcastsi128_si256(hi128);
        let mask = _mm256_set1_epi8(0x0F);
        (lo, hi, mask)
    }
}

// _mm256_srli_epi64 shifts 64-bit lanes (not bytes), so the high-nibble path
// AND-masks after the shift to clear bits pulled in from the next byte.
#[target_feature(enable = "avx2")]
#[inline]
fn lookup(lo: __m256i, hi: __m256i, mask: __m256i, x: __m256i) -> __m256i {
    let x_lo = _mm256_and_si256(x, mask);
    let x_hi = _mm256_and_si256(_mm256_srli_epi64::<4>(x), mask);
    let r_lo = _mm256_shuffle_epi8(lo, x_lo);
    let r_hi = _mm256_shuffle_epi8(hi, x_hi);
    _mm256_xor_si256(r_lo, r_hi)
}

#[cfg(test)]
mod tests {
    use super::super::scalar;
    use super::*;

    #[test]
    fn matches_scalar_exhaustive() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }
        let lengths = [
            0usize, 1, 15, 31, 32, 33, 63, 64, 65, 95, 96, 127, 128, 129, 255, 256, 1024,
        ];
        let input: Vec<u8> = (0..1024).map(|i| (i & 0xFF) as u8).collect();

        for c in 0..=255u8 {
            for &n in &lengths {
                let mut out_simd = vec![0u8; n];
                let mut out_scalar = vec![0u8; n];
                // SAFETY: CPUID-checked above.
                unsafe { mul_slice(c, &input[..n], &mut out_simd) };
                scalar::mul_slice(c, &input[..n], &mut out_scalar);
                assert_eq!(out_simd, out_scalar, "mul_slice c={c} n={n}");

                let seed: Vec<u8> = (0..n).map(|i| (i as u8) ^ 0x5A).collect();
                let mut xor_simd = seed.clone();
                let mut xor_scalar = seed.clone();
                // SAFETY: CPUID-checked above.
                unsafe { mul_slice_xor(c, &input[..n], &mut xor_simd) };
                scalar::mul_slice_xor(c, &input[..n], &mut xor_scalar);
                assert_eq!(xor_simd, xor_scalar, "mul_slice_xor c={c} n={n}");
            }
        }
    }
}
