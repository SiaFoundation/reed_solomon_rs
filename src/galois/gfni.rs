//! GFNI slice multiply via `vgf2p8affineqb`. See klauspost's `_gfni` paths in
//! [`galois_gen_amd64.s`]. Uses the VEX-256 encoding (GFNI + AVX2, no AVX-512).
//!
//! [`galois_gen_amd64.s`]: https://github.com/klauspost/reedsolomon/blob/master/galois_gen_amd64.s

use core::arch::x86_64::*;

use super::MUL_TABLE;
use super::simd::slice_loop;

/// Per-coefficient 8x8 GF(2) matrix packed into one u64 in the layout
/// `vgf2p8affineqb` expects. See [Intel intrinsics guide].
///
/// [Intel intrinsics guide]: https://www.intel.com/content/www/us/en/docs/intrinsics-guide/index.html#text=_mm256_gf2p8affine_epi64_epi8
static GFNI_MATRICES: [u64; 256] = build_gfni_matrices();

const fn build_gfni_matrices() -> [u64; 256] {
    let mut out = [0u64; 256];
    let mut c: usize = 0;
    while c < 256 {
        let mut m: u64 = 0;
        // Column j of the 8x8 GF(2) matrix is mul(c, 2^j). gf2p8affineqb reads
        // row i from byte (7-i) of the qword, so column j contributes to byte
        // (7-k) bit j when bit k of mul(c, 2^j) is set.
        let mut v: u8 = c as u8;
        let mut j: usize = 0;
        while j < 8 {
            let mut k: usize = 0;
            while k < 8 {
                if (v >> k) & 1 == 1 {
                    m |= 1u64 << ((7 - k) * 8 + j);
                }
                k += 1;
            }
            // v *= 2 in GF(2^8); fold via 0x1D on overflow.
            let hi = v & 0x80;
            v <<= 1;
            if hi != 0 {
                v ^= 0x1D;
            }
            j += 1;
        }
        out[c] = m;
        c += 1;
    }
    out
}

const VEC_BYTES: usize = 32;
const UNROLL: usize = 2;

#[target_feature(enable = "gfni,avx,avx2")]
pub(super) fn mul_slice(coeff: u8, input: &[u8], out: &mut [u8]) {
    let m = _mm256_set1_epi64x(GFNI_MATRICES[coeff as usize] as i64);

    slice_loop::<VEC_BYTES, UNROLL>(
        input,
        out,
        // SAFETY: slice_loop guarantees VEC_BYTES at p_in / p_out.
        |p_in, p_out| unsafe {
            let v = _mm256_loadu_si256(p_in.cast());
            let r = _mm256_gf2p8affine_epi64_epi8::<0>(v, m);
            _mm256_storeu_si256(p_out.cast(), r);
        },
        |in_t, out_t| {
            let table = &MUL_TABLE[coeff as usize];
            for (o, &x) in out_t.iter_mut().zip(in_t) {
                *o = table[x as usize];
            }
        },
    );
}

#[target_feature(enable = "gfni,avx,avx2")]
pub(super) fn mul_slice_xor(coeff: u8, input: &[u8], out: &mut [u8]) {
    let m = _mm256_set1_epi64x(GFNI_MATRICES[coeff as usize] as i64);

    slice_loop::<VEC_BYTES, UNROLL>(
        input,
        out,
        // SAFETY: slice_loop guarantees VEC_BYTES at p_in / p_out.
        |p_in, p_out| unsafe {
            let p_in = p_in.cast::<__m256i>();
            let p_out = p_out.cast::<__m256i>();
            let v = _mm256_loadu_si256(p_in);
            let e = _mm256_loadu_si256(p_out);
            let r = _mm256_gf2p8affine_epi64_epi8::<0>(v, m);
            _mm256_storeu_si256(p_out, _mm256_xor_si256(e, r));
        },
        |in_t, out_t| {
            let table = &MUL_TABLE[coeff as usize];
            for (o, &x) in out_t.iter_mut().zip(in_t) {
                *o ^= table[x as usize];
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::super::scalar;
    use super::*;

    #[test]
    fn matches_scalar_exhaustive() {
        if !is_x86_feature_detected!("gfni")
            || !is_x86_feature_detected!("avx2")
            || !is_x86_feature_detected!("avx")
        {
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
