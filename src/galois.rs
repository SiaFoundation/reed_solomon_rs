//! GF(2^8) arithmetic, ported from klauspost/reedsolomon
//! ([`galois.go`](https://github.com/klauspost/reedsolomon/blob/master/galois.go)).
//! Polynomial 0x11D, tables built at compile time. SIMD backends in submodules;
//! the public `mul_slice` / `mul_slice_xor` dispatch at first call.

mod scalar;

const GENERATING_POLYNOMIAL: u16 = 0x11D;

const fn build_log_exp() -> ([u8; 256], [u8; 256]) {
    let mut log = [0u8; 256];
    let mut exp = [0u8; 256];

    let mut x: u16 = 1;
    let mut i = 0;
    while i < 255 {
        exp[i] = x as u8;
        log[x as usize] = i as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= GENERATING_POLYNOMIAL;
        }
        i += 1;
    }
    // Sentinel so `inv(1)` can index EXP[255] without a wrap-around check.
    exp[255] = exp[0];
    (log, exp)
}

const TABLES: ([u8; 256], [u8; 256]) = build_log_exp();

pub(crate) const LOG_TABLE: [u8; 256] = TABLES.0;
pub(crate) const EXP_TABLE: [u8; 256] = TABLES.1;

/// GF(2^8) multiply. See klauspost's [`galMultiply`].
///
/// [`galMultiply`]: https://github.com/klauspost/reedsolomon/blob/master/galois.go
pub(crate) const fn mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    let la = LOG_TABLE[a as usize] as u16;
    let lb = LOG_TABLE[b as usize] as u16;
    let sum = la + lb;
    // sum in [0, 508]; one subtract equals `sum % 255` without the division.
    let idx = if sum >= 255 { sum - 255 } else { sum };
    EXP_TABLE[idx as usize]
}

/// MUL_TABLE[a][b] = mul(a, b).
const fn build_mul_table() -> [[u8; 256]; 256] {
    let mut table = [[0u8; 256]; 256];
    let mut a: usize = 0;
    while a < 256 {
        let mut b: usize = 0;
        while b < 256 {
            table[a][b] = mul(a as u8, b as u8);
            b += 1;
        }
        a += 1;
    }
    table
}

pub(crate) static MUL_TABLE: [[u8; 256]; 256] = build_mul_table();

// Shared by the SIMD backends; gated together so nothing here is compiled
// when no backend will consume it.
#[cfg(any(
    all(target_arch = "x86_64", feature = "simd"),
    all(
        any(target_arch = "aarch64", target_arch = "arm64ec"),
        target_feature = "neon",
        feature = "simd",
    ),
))]
mod simd {
    use super::mul;

    /// Nibble-split lookup tables for VPSHUFB / VTBL. Per coefficient: 16
    /// bytes for the low-nibble lookup, 16 for the high-nibble lookup. The
    /// xor of the two is `mul(c, x)`.
    pub(super) static NIBBLE_TABLES: [[u8; 32]; 256] = build_nibble_tables();

    const fn build_nibble_tables() -> [[u8; 32]; 256] {
        let mut t = [[0u8; 32]; 256];
        let mut c: usize = 0;
        while c < 256 {
            let mut n: usize = 0;
            while n < 16 {
                t[c][n] = mul(c as u8, n as u8);
                t[c][16 + n] = mul(c as u8, (n as u8) << 4);
                n += 1;
            }
            c += 1;
        }
        t
    }

    /// Calls `one(p_in, p_out)` per VEC-byte chunk, unrolled UNROLL times,
    /// then once for any remaining whole VEC, then hands the byte tail to
    /// `tail`. `#[inline(always)]` so the closure inherits the caller's
    /// target_feature context.
    #[inline(always)]
    pub(super) fn slice_loop<const VEC: usize, const UNROLL: usize>(
        input: &[u8],
        out: &mut [u8],
        mut one: impl FnMut(*const u8, *mut u8),
        tail: impl FnOnce(&[u8], &mut [u8]),
    ) {
        debug_assert_eq!(input.len(), out.len());
        let stride = VEC * UNROLL;

        let mut in_u = input.chunks_exact(stride);
        let mut out_u = out.chunks_exact_mut(stride);
        for (in_block, out_block) in (&mut in_u).zip(&mut out_u) {
            let p_in = in_block.as_ptr();
            let p_out = out_block.as_mut_ptr();
            for k in 0..UNROLL {
                // SAFETY: chunks_exact returned exactly VEC*UNROLL bytes.
                let off = k * VEC;
                one(unsafe { p_in.add(off) }, unsafe { p_out.add(off) });
            }
        }

        let mut in_s = in_u.remainder().chunks_exact(VEC);
        let mut out_s = out_u.into_remainder().chunks_exact_mut(VEC);
        for (in_block, out_block) in (&mut in_s).zip(&mut out_s) {
            one(in_block.as_ptr(), out_block.as_mut_ptr());
        }

        tail(in_s.remainder(), out_s.into_remainder());
    }

    #[cfg(test)]
    mod tests {
        use super::super::mul;
        use super::NIBBLE_TABLES;

        #[test]
        fn nibble_tables_match_mul() {
            for c in 0..=255u8 {
                let t = &NIBBLE_TABLES[c as usize];
                for x in 0..=255u8 {
                    let lo = t[(x & 0x0F) as usize];
                    let hi = t[16 + ((x >> 4) & 0x0F) as usize];
                    assert_eq!(lo ^ hi, mul(c, x), "c={c} x={x}");
                }
            }
        }
    }
}

/// Multiplicative inverse in GF(2^8). Returns 0 for a = 0 (undefined).
pub(crate) const fn inv(a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    let la = LOG_TABLE[a as usize] as u16;
    let idx = 255 - la;
    EXP_TABLE[idx as usize]
}

const fn build_inv_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut i = 1;
    while i < 256 {
        table[i] = inv(i as u8);
        i += 1;
    }
    table
}

pub(crate) const INV_TABLE: [u8; 256] = build_inv_table();

/// GF(2^8) exponentiation. See klauspost's [`galExp`] for the 0^0 = 1
/// convention this depends on (used by Vandermonde row construction).
///
/// [`galExp`]: https://github.com/klauspost/reedsolomon/blob/master/galois.go
pub(crate) const fn exp(a: u8, n: usize) -> u8 {
    if n == 0 {
        return 1;
    }
    if a == 0 {
        return 0;
    }
    let log_a = LOG_TABLE[a as usize] as usize;
    EXP_TABLE[(log_a * n) % 255]
}

cfg_if::cfg_if! {
    if #[cfg(all(target_arch = "x86_64", feature = "simd"))] {
        mod avx2;
        mod gfni;

        pub(crate) fn mul_slice(coeff: u8, input: &[u8], out: &mut [u8]) {
            debug_assert_eq!(input.len(), out.len());
            unsafe {
                if is_x86_feature_detected!("gfni")
                    && is_x86_feature_detected!("avx2")
                    && is_x86_feature_detected!("avx")
                {
                    return gfni::mul_slice(coeff, input, out);
                } else if is_x86_feature_detected!("avx2") {
                    return avx2::mul_slice(coeff, input, out);
                }
            }
            scalar::mul_slice(coeff, input, out)
        }

        pub(crate) fn mul_slice_xor(coeff: u8, input: &[u8], out: &mut [u8]) {
            debug_assert_eq!(input.len(), out.len());
            unsafe {
                if is_x86_feature_detected!("gfni") && is_x86_feature_detected!("avx2") {
                    return gfni::mul_slice_xor(coeff, input, out);
                } else if is_x86_feature_detected!("avx2") {
                    return avx2::mul_slice_xor(coeff, input, out);
                }
            }
            scalar::mul_slice_xor(coeff, input, out)
        }
    } else if #[cfg(all(
        any(target_arch = "aarch64", target_arch = "arm64ec"),
        target_feature = "neon",
        feature = "simd",
    ))] {
        mod neon;

        pub(crate) fn mul_slice(coeff: u8, input: &[u8], out: &mut [u8]) {
            debug_assert_eq!(input.len(), out.len());
            neon::mul_slice(coeff, input, out)
        }

        pub(crate) fn mul_slice_xor(coeff: u8, input: &[u8], out: &mut [u8]) {
            debug_assert_eq!(input.len(), out.len());
            neon::mul_slice_xor(coeff, input, out)
        }
    } else {
        pub(crate) fn mul_slice(coeff: u8, input: &[u8], out: &mut [u8]) {
            debug_assert_eq!(input.len(), out.len());
            scalar::mul_slice(coeff, input, out)
        }

        pub(crate) fn mul_slice_xor(coeff: u8, input: &[u8], out: &mut [u8]) {
            debug_assert_eq!(input.len(), out.len());
            scalar::mul_slice_xor(coeff, input, out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_table_matches_mul() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                assert_eq!(MUL_TABLE[a as usize][b as usize], mul(a, b));
            }
        }
    }

    #[test]
    fn inv_round_trip() {
        for a in 1..=255u8 {
            let i = INV_TABLE[a as usize];
            assert_eq!(mul(a, i), 1, "a={a} inv={i}");
        }
    }

    // klauspost defines 0^0 = 1 and 0^n = 0; the Vandermonde row
    // construction in matrix.rs depends on both.
    #[test]
    fn exp_klauspost_conventions() {
        assert_eq!(exp(0, 0), 1);
        assert_eq!(exp(0, 1), 0);
        // 2^8 reduces to 0x1D, the low byte of the generator polynomial.
        assert_eq!(exp(2, 8), GENERATING_POLYNOMIAL as u8);
    }

    // Anchor values reproduced from klauspost on the same field.
    #[test]
    fn known_vectors() {
        assert_eq!(LOG_TABLE[1], 0);
        assert_eq!(LOG_TABLE[2], 1);
        assert_eq!(EXP_TABLE[0], 1);
        assert_eq!(EXP_TABLE[1], 2);
        assert_eq!(EXP_TABLE[8], GENERATING_POLYNOMIAL as u8);
        assert_eq!(mul(2, 3), 6);
        assert_eq!(mul(2, 2), 4);
        assert_eq!(mul(0x80, 2), GENERATING_POLYNOMIAL as u8);
        assert_eq!(mul(0x53, INV_TABLE[0x53]), 1);
        assert_eq!(mul(0xCA, INV_TABLE[0xCA]), 1);
    }

    #[test]
    fn dispatch_matches_scalar_all_coeffs() {
        let lengths = [0usize, 1, 7, 15, 16, 17, 31, 32, 33, 63, 64, 100, 257, 1024];
        let input: Vec<u8> = (0..1024u32).map(|x| (x ^ (x >> 3)) as u8).collect();
        for &n in &lengths {
            for coeff in [0u8, 1, 2, 3, 5, 7, 13, 17, 31, 100, 128, 200, 254, 255] {
                let mut out_dispatch = vec![0u8; n];
                let mut out_scalar = vec![0u8; n];
                mul_slice(coeff, &input[..n], &mut out_dispatch);
                scalar::mul_slice(coeff, &input[..n], &mut out_scalar);
                assert_eq!(out_dispatch, out_scalar, "mul_slice: coeff={coeff} n={n}");

                let mut xor_dispatch: Vec<u8> = (0..n).map(|i| (i ^ 0xA5) as u8).collect();
                let mut xor_scalar = xor_dispatch.clone();
                mul_slice_xor(coeff, &input[..n], &mut xor_dispatch);
                scalar::mul_slice_xor(coeff, &input[..n], &mut xor_scalar);
                assert_eq!(
                    xor_dispatch, xor_scalar,
                    "mul_slice_xor: coeff={coeff} n={n}"
                );
            }
        }
    }
}
