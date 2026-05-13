//! Row-major matrix over GF(2^8). Translation of `matrix.go` from klauspost.

use crate::error::{Error, Result};
use crate::galois::{INV_TABLE, MUL_TABLE, exp, mul};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<u8>,
}

impl Matrix {
    pub fn zero(rows: usize, cols: usize) -> Self {
        assert!(rows > 0 && cols > 0, "matrix dimensions must be positive");
        Self {
            rows,
            cols,
            data: vec![0u8; rows * cols],
        }
    }

    pub fn identity(n: usize) -> Self {
        let mut m = Self::zero(n, n);
        for i in 0..n {
            m.set(i, i, 1);
        }
        m
    }

    /// `V · T⁻¹` where `V[r][c] = r^c` and `T` is the top `k × k` square of
    /// `V`. Top of the result is identity; bottom is the parity coefficients.
    /// Matches klauspost's `buildMatrix`.
    pub fn vandermonde_encoding(data_shards: usize, total_shards: usize) -> Result<Self> {
        let v = Self::vandermonde_raw(total_shards, data_shards);
        let top = v.sub_matrix(0, 0, data_shards, data_shards);
        let top_inv = top.invert()?;
        v.multiply(&top_inv)
    }

    fn vandermonde_raw(rows: usize, cols: usize) -> Self {
        let mut m = Self::zero(rows, cols);
        for r in 0..rows {
            for c in 0..cols {
                m.set(r, c, exp(r as u8, c));
            }
        }
        m
    }

    /// Identity over the top `k × k`; bottom is `INV_TABLE[r ^ c]`. Cheaper
    /// to build than Vandermonde (no inverse) but produces different parity
    /// bytes — not wire-compatible.
    pub fn cauchy(data_shards: usize, total_shards: usize) -> Self {
        let mut m = Self::zero(total_shards, data_shards);
        for r in 0..total_shards {
            for c in 0..data_shards {
                let v = if r < data_shards {
                    if r == c { 1 } else { 0 }
                } else {
                    INV_TABLE[(r ^ c) & 0xff]
                };
                m.set(r, c, v);
            }
        }
        m
    }

    #[allow(dead_code)]
    #[inline(always)]
    pub fn rows(&self) -> usize {
        self.rows
    }

    #[allow(dead_code)]
    #[inline(always)]
    pub fn cols(&self) -> usize {
        self.cols
    }

    #[inline(always)]
    pub fn get(&self, r: usize, c: usize) -> u8 {
        self.data[r * self.cols + c]
    }

    #[inline(always)]
    pub fn set(&mut self, r: usize, c: usize, v: u8) {
        self.data[r * self.cols + c] = v;
    }

    #[inline(always)]
    pub fn row(&self, r: usize) -> &[u8] {
        let start = r * self.cols;
        &self.data[start..start + self.cols]
    }

    #[allow(dead_code)]
    pub fn multiply(&self, other: &Matrix) -> Result<Matrix> {
        if self.cols != other.rows {
            return Err(Error::SingularMatrix);
        }
        let mut result = Matrix::zero(self.rows, other.cols);
        for r in 0..self.rows {
            for c in 0..other.cols {
                let mut v: u8 = 0;
                for i in 0..self.cols {
                    v ^= MUL_TABLE[self.get(r, i) as usize][other.get(i, c) as usize];
                }
                result.set(r, c, v);
            }
        }
        Ok(result)
    }

    /// `[self | right]` — horizontal concatenation.
    pub fn augment(&self, right: &Matrix) -> Matrix {
        assert_eq!(self.rows, right.rows, "augment row count mismatch");
        let new_cols = self.cols + right.cols;
        let mut out = Matrix::zero(self.rows, new_cols);
        for r in 0..self.rows {
            for c in 0..self.cols {
                out.set(r, c, self.get(r, c));
            }
            for c in 0..right.cols {
                out.set(r, self.cols + c, right.get(r, c));
            }
        }
        out
    }

    pub fn sub_matrix(&self, rmin: usize, cmin: usize, rmax: usize, cmax: usize) -> Matrix {
        let mut out = Matrix::zero(rmax - rmin, cmax - cmin);
        for r in rmin..rmax {
            for c in cmin..cmax {
                out.set(r - rmin, c - cmin, self.get(r, c));
            }
        }
        out
    }

    fn swap_rows(&mut self, r1: usize, r2: usize) {
        if r1 == r2 {
            return;
        }
        for c in 0..self.cols {
            let a = self.get(r1, c);
            let b = self.get(r2, c);
            self.set(r1, c, b);
            self.set(r2, c, a);
        }
    }

    fn is_square(&self) -> bool {
        self.rows == self.cols
    }

    pub fn invert(&self) -> Result<Matrix> {
        if !self.is_square() {
            return Err(Error::SingularMatrix);
        }
        let n = self.rows;
        let mut work = self.augment(&Matrix::identity(n));
        work.gaussian_elimination()?;
        Ok(work.sub_matrix(0, n, n, 2 * n))
    }

    fn gaussian_elimination(&mut self) -> Result<()> {
        let rows = self.rows;
        let cols = self.cols;

        // Forward: clear below the diagonal, scale the diagonal to 1.
        for r in 0..rows {
            if self.get(r, r) == 0 {
                for rb in (r + 1)..rows {
                    if self.get(rb, r) != 0 {
                        self.swap_rows(r, rb);
                        break;
                    }
                }
            }
            if self.get(r, r) == 0 {
                return Err(Error::SingularMatrix);
            }
            if self.get(r, r) != 1 {
                let scale = INV_TABLE[self.get(r, r) as usize];
                for c in 0..cols {
                    let v = self.get(r, c);
                    self.set(r, c, mul(v, scale));
                }
            }
            // Zero column r below the diagonal. XOR == subtraction in GF(2^8).
            for rb in (r + 1)..rows {
                if self.get(rb, r) != 0 {
                    let scale = self.get(rb, r);
                    let pivot_row_start = r * cols;
                    for c in 0..cols {
                        let pv = self.data[pivot_row_start + c];
                        let v = self.get(rb, c) ^ MUL_TABLE[scale as usize][pv as usize];
                        self.set(rb, c, v);
                    }
                }
            }
        }

        // Backward: clear above the diagonal.
        for d in 0..rows {
            for ra in 0..d {
                if self.get(ra, d) != 0 {
                    let scale = self.get(ra, d);
                    let pivot_row_start = d * cols;
                    for c in 0..cols {
                        let pv = self.data[pivot_row_start + c];
                        let v = self.get(ra, c) ^ MUL_TABLE[scale as usize][pv as usize];
                        self.set(ra, c, v);
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_inverse_is_self() {
        let i = Matrix::identity(5);
        let inv = i.invert().unwrap();
        assert_eq!(i, inv);
    }

    #[test]
    fn invert_round_trip() {
        let full = Matrix::cauchy(4, 8);
        let parity = full.sub_matrix(4, 0, 8, 4);
        let inv = parity.invert().unwrap();
        let product = parity.multiply(&inv).unwrap();
        assert_eq!(product, Matrix::identity(4));
    }

    #[test]
    fn vandermonde_top_is_identity() {
        let m = Matrix::vandermonde_encoding(5, 10).unwrap();
        for r in 0..5 {
            for c in 0..5 {
                assert_eq!(m.get(r, c), if r == c { 1 } else { 0 });
            }
        }
    }

    #[test]
    fn vandermonde_any_square_submatrix_is_invertible() {
        let data = 4;
        let m = Matrix::vandermonde_encoding(data, data + 6).unwrap();
        let mut sub = Matrix::zero(data, data);
        for (i, &r) in [1usize, 3, 7, 9].iter().enumerate() {
            for c in 0..data {
                sub.set(i, c, m.get(r, c));
            }
        }
        sub.invert()
            .expect("Vandermonde submatrix must be invertible");
    }

    #[test]
    fn cauchy_top_is_identity() {
        let m = Matrix::cauchy(6, 10);
        for r in 0..6 {
            for c in 0..6 {
                assert_eq!(m.get(r, c), if r == c { 1 } else { 0 });
            }
        }
    }

    #[test]
    fn cauchy_any_square_submatrix_is_invertible() {
        let data = 4;
        let parity = 6;
        let m = Matrix::cauchy(data, data + parity);

        // Rows [0, 2, 5, 8] — mix of identity and Cauchy rows.
        let mut sub = Matrix::zero(data, data);
        for (i, &r) in [0usize, 2, 5, 8].iter().enumerate() {
            for c in 0..data {
                sub.set(i, c, m.get(r, c));
            }
        }
        sub.invert().expect("submatrix must be invertible");
    }

    #[test]
    fn singular_matrix_detected() {
        // Two identical rows => singular.
        let mut m = Matrix::identity(3);
        m.set(1, 1, 0);
        m.set(1, 0, 1);
        assert!(matches!(m.invert(), Err(Error::SingularMatrix)));
    }

    #[test]
    fn multiply_with_identity_is_self() {
        let m = Matrix::cauchy(3, 5);
        let i = Matrix::identity(3);
        let prod = m.multiply(&i).unwrap();
        assert_eq!(prod, m);
    }
}
