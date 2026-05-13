//! GF(2^8) with primitive element 2 and generator polynomial 0x11D —
//! the field klauspost/reedsolomon uses. Tables baked in via `const fn`.
//!
//! Translation of the field code in klauspost/reedsolomon's
//! [`galois.go`](https://github.com/klauspost/reedsolomon/blob/master/galois.go):
//! `galLogTable` / `galExpTable` initialization, `galMultiply`, `galMulSlice`,
//! and `galMulSliceXor`.

const GENERATING_POLYNOMIAL: u16 = 0x11D;

/// `EXP[i] = 2^i mod (255)`, `LOG[x]` is the inverse (undefined at x = 0).
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
    // Sentinel: `inv(1)` evaluates `EXP[255 - log[1]] = EXP[255]`. Mirroring
    // exp[0] = 1 here lets `inv` skip a wrap-around check on the hot path.
    exp[255] = exp[0];
    (log, exp)
}

const TABLES: ([u8; 256], [u8; 256]) = build_log_exp();

pub(crate) const LOG_TABLE: [u8; 256] = TABLES.0;
pub(crate) const EXP_TABLE: [u8; 256] = TABLES.1;

/// `a * b` in GF(2^8). Matches klauspost's `galMultiply` in `galois.go`.
/// Zero is handled explicitly since `log(0)` is undefined.
pub(crate) const fn mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    let la = LOG_TABLE[a as usize] as u16;
    let lb = LOG_TABLE[b as usize] as u16;
    let sum = la + lb;
    // la, lb ∈ [0, 254] so sum ∈ [0, 508]; subtract the multiplicative-group
    // order (255) once when sum ≥ 255. Equivalent to `sum % 255` but avoids
    // the division at every multiply.
    let idx = if sum >= 255 { sum - 255 } else { sum };
    EXP_TABLE[idx as usize]
}

/// Builds the 256x256 multiplication table.
///
/// `MUL_TABLE[a][b] = a * b` in GF(2^8). 64 KiB; the encode hot path indexes
/// `MUL_TABLE[coeff]` once per (matrix-row, data-shard) pair and then walks
/// the resulting 256-byte table inside an inner loop, which keeps it in L1.
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

/// `1 / a` in GF(2^8). Undefined (returns 0) for a = 0.
pub(crate) const fn inv(a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    // 1/a = exp[(255 - log[a]) mod 255]
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

/// `a^n` in GF(2^8). Matches `galExp` from klauspost/reedsolomon
/// (`galois.go`):
///
///   - `a^0 == 1` for all `a` (including `0^0 == 1`).
///   - `0^n == 0` for `n > 0`.
///   - Otherwise computed via `EXP[(LOG[a] * n) mod 255]`.
///
/// Used to construct Vandermonde matrices, where row `r` column `c` is `r^c`.
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

/// `out[i] = MUL_TABLE[coeff][input[i]]`. Matches `galMulSlice` from
/// klauspost's `galois.go`. Used for the first contribution to a parity
/// shard so we can skip XOR with an undefined buffer.
#[inline(always)]
pub(crate) fn mul_slice(coeff: u8, input: &[u8], out: &mut [u8]) {
    debug_assert_eq!(input.len(), out.len());
    let table = &MUL_TABLE[coeff as usize];
    for (o, &x) in out.iter_mut().zip(input.iter()) {
        *o = table[x as usize];
    }
}

/// `out[i] ^= MUL_TABLE[coeff][input[i]]`. Matches `galMulSliceXor` from
/// klauspost's `galois.go`. The hot loop of the encoder.
#[inline(always)]
pub(crate) fn mul_slice_xor(coeff: u8, input: &[u8], out: &mut [u8]) {
    debug_assert_eq!(input.len(), out.len());
    let table = &MUL_TABLE[coeff as usize];
    for (o, &x) in out.iter_mut().zip(input.iter()) {
        *o ^= table[x as usize];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_zero() {
        for x in 0..=255u8 {
            assert_eq!(mul(0, x), 0);
            assert_eq!(mul(x, 0), 0);
        }
    }

    #[test]
    fn mul_one_is_identity() {
        for x in 0..=255u8 {
            assert_eq!(mul(1, x), x);
            assert_eq!(mul(x, 1), x);
        }
    }

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

    #[test]
    fn mul_is_commutative_and_associative() {
        // Spot-check; full enumeration is 16M ops which is overkill.
        let triples = [
            (1u8, 2, 3),
            (5, 17, 31),
            (200, 150, 100),
            (255, 254, 253),
            (128, 64, 32),
        ];
        for (a, b, c) in triples {
            assert_eq!(mul(a, b), mul(b, a));
            assert_eq!(mul(mul(a, b), c), mul(a, mul(b, c)));
        }
    }

    #[test]
    fn exp_edge_cases() {
        // 0^0 = 1 by klauspost convention.
        assert_eq!(exp(0, 0), 1);
        // 0^n = 0 for n > 0.
        for n in 1..10 {
            assert_eq!(exp(0, n), 0);
        }
        // a^0 = 1 for all a.
        for a in 0..=255u8 {
            assert_eq!(exp(a, 0), 1);
        }
        // a^1 = a.
        for a in 0..=255u8 {
            assert_eq!(exp(a, 1), a);
        }
        // 2^2 = 4 (no reduction needed).
        assert_eq!(exp(2, 2), 4);
        // 2^8 hits the polynomial — equals 0x1D (= generator low byte).
        assert_eq!(exp(2, 8), GENERATING_POLYNOMIAL as u8);
    }

    #[test]
    fn known_vectors() {
        // A handful of values produced by Klaus Post's reedsolomon library on
        // the same field; if these match we have the right tables.
        assert_eq!(LOG_TABLE[1], 0);
        assert_eq!(LOG_TABLE[2], 1);
        assert_eq!(EXP_TABLE[0], 1);
        assert_eq!(EXP_TABLE[1], 2);
        assert_eq!(EXP_TABLE[8], GENERATING_POLYNOMIAL as u8); // 0x1D
        // 2 * 3 = 2 + 3 in poly form (carry-less since neither has the high
        // bit) — straight XOR-add of x and x+1 = x*(x+1) = x^2+x = 6.
        assert_eq!(mul(2, 3), 6);
        // 2 * 2 = x*x = x^2 = 4.
        assert_eq!(mul(2, 2), 4);
        // 0x80 * 2 = polynomial reduction kicks in (x^7 * x = x^8 ≡ 0x1D).
        assert_eq!(mul(0x80, 2), GENERATING_POLYNOMIAL as u8);
        // a * (1/a) = 1.
        assert_eq!(mul(0x53, INV_TABLE[0x53]), 1);
        assert_eq!(mul(0xCA, INV_TABLE[0xCA]), 1);
    }

    #[test]
    fn mul_slice_matches_scalar() {
        let input: Vec<u8> = (0..200u32).map(|x| x as u8).collect();
        let mut out_simd = vec![0u8; input.len()];
        let mut out_scalar = vec![0u8; input.len()];
        for coeff in [0u8, 1, 2, 7, 13, 100, 255] {
            mul_slice(coeff, &input, &mut out_simd);
            for (o, &x) in out_scalar.iter_mut().zip(&input) {
                *o = mul(coeff, x);
            }
            assert_eq!(out_simd, out_scalar, "coeff={coeff}");
        }
    }

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
