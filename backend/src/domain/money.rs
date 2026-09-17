//! Pure Rust integer Indonesian Rupiah value object with zero floating-point arithmetic loss.
//!
//! # Invariants:
//! 1. 1 unit = Rp 1 (no fractional subunits in circulating Indonesian currency).
//! 2. Stored internally as signed 64-bit integer (`i64`), supporting $\pm 9.22 \times 10^{18}$ IDR.
//! 3. Zero floating-point math: absolute prohibition of `f32`/`f64` conversions.
//! 4. Transparent Serde JSON serialization as native numeric integer.
//! 5. Direct SQLite `INTEGER` storage binding via SQLx Type/Encode/Decode.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, Neg, Sub};

/// Strongly typed integer Indonesian Rupiah value object.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    sqlx::Type,
)]
#[serde(transparent)]
#[sqlx(transparent)]
pub struct Rupiah(pub i64);

impl Rupiah {
    /// Zero Rupiah constant: Rp 0
    pub const ZERO: Rupiah = Rupiah(0);

    /// Minimum possible Rupiah value: -9,223,372,036,854,775,808
    pub const MIN: Rupiah = Rupiah(i64::MIN);

    /// Maximum possible Rupiah value: +9,223,372,036,854,775,807
    pub const MAX: Rupiah = Rupiah(i64::MAX);

    /// Create a new Rupiah instance from an `i64` integer.
    #[inline]
    pub const fn new(amount: i64) -> Self {
        Self(amount)
    }

    /// Return the inner `i64` integer value.
    #[inline]
    pub const fn as_i64(&self) -> i64 {
        self.0
    }

    /// Consume self and return the inner `i64` integer value.
    #[inline]
    pub const fn into_inner(self) -> i64 {
        self.0
    }

    /// Check if the amount is exactly zero.
    #[inline]
    pub const fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// Check if the amount is strictly positive (> 0).
    #[inline]
    pub const fn is_positive(&self) -> bool {
        self.0 > 0
    }

    /// Check if the amount is strictly negative (< 0).
    #[inline]
    pub const fn is_negative(&self) -> bool {
        self.0 < 0
    }

    /// Checked addition. Returns `None` on arithmetic overflow.
    #[inline]
    pub fn checked_add(&self, other: Rupiah) -> Option<Rupiah> {
        self.0.checked_add(other.0).map(Rupiah)
    }

    /// Checked subtraction. Returns `None` on arithmetic underflow.
    #[inline]
    pub fn checked_sub(&self, other: Rupiah) -> Option<Rupiah> {
        self.0.checked_sub(other.0).map(Rupiah)
    }

    /// Checked multiplication by an integer scalar. Returns `None` on overflow.
    ///
    /// Note: Money is multiplied by a dimensionless scalar (e.g. count, months),
    /// never by another currency unit.
    #[inline]
    pub fn checked_mul(&self, scalar: i64) -> Option<Rupiah> {
        self.0.checked_mul(scalar).map(Rupiah)
    }

    /// Checked division by an integer divisor.
    ///
    /// Returns `None` if `divisor == 0` or if operation overflows (`i64::MIN / -1`).
    /// Division truncates towards zero.
    #[inline]
    pub fn checked_div(&self, divisor: i64) -> Option<Rupiah> {
        if divisor == 0 {
            None
        } else {
            self.0.checked_div(divisor).map(Rupiah)
        }
    }

    /// Checked division returning both quotient and remainder `(quotient, remainder)`.
    ///
    /// Invariant: `quotient * divisor + remainder == original`.
    /// Useful for bill splitting without losing 1 Rupiah.
    pub fn checked_div_rem(&self, divisor: i64) -> Option<(Rupiah, Rupiah)> {
        if divisor == 0 {
            return None;
        }
        let q = self.0.checked_div(divisor)?;
        let r = self.0.checked_rem(divisor)?;
        Some((Rupiah(q), Rupiah(r)))
    }

    /// Checked negation. Returns `None` if self is `Rupiah::MIN`.
    #[inline]
    pub fn checked_neg(&self) -> Option<Rupiah> {
        self.0.checked_neg().map(Rupiah)
    }

    /// Checked absolute value. Returns `None` if self is `Rupiah::MIN`.
    #[inline]
    pub fn abs(&self) -> Option<Rupiah> {
        self.0.checked_abs().map(Rupiah)
    }

    /// Saturating addition. Clamps to `Rupiah::MAX` or `Rupiah::MIN` on overflow.
    #[inline]
    pub fn saturating_add(&self, other: Rupiah) -> Rupiah {
        Rupiah(self.0.saturating_add(other.0))
    }

    /// Saturating subtraction. Clamps to `Rupiah::MAX` or `Rupiah::MIN` on underflow.
    #[inline]
    pub fn saturating_sub(&self, other: Rupiah) -> Rupiah {
        Rupiah(self.0.saturating_sub(other.0))
    }

    /// Integer basis-points multiplication (1 bps = 0.01% = 1/10,000).
    ///
    /// Example: 11% PPN (VAT) = 1,100 bps.
    /// Rp 100,000 * 1,100 bps / 10,000 = Rp 11,000.
    pub fn checked_mul_bps(&self, bps: i64) -> Option<Rupiah> {
        self.0.checked_mul(bps)?.checked_div(10_000).map(Rupiah)
    }

    /// Integer ratio calculation: `(self * numerator) / denominator`.
    pub fn checked_mul_ratio(&self, numerator: i64, denominator: i64) -> Option<Rupiah> {
        if denominator == 0 {
            None
        } else {
            self.0
                .checked_mul(numerator)?
                .checked_div(denominator)
                .map(Rupiah)
        }
    }

    /// Sum an iterator of `Rupiah` safely, returning `None` if overflow occurs.
    pub fn checked_sum<I: IntoIterator<Item = Rupiah>>(iter: I) -> Option<Rupiah> {
        iter.into_iter()
            .try_fold(Rupiah::ZERO, |acc, x| acc.checked_add(x))
    }

    /// Format to Indonesian Rupiah standard display string.
    ///
    /// Formats as `"Rp X.XXX.XXX"`, with period thousands separators and
    /// leading minus sign for negative amounts (`"-Rp X.XXX.XXX"`).
    /// Guaranteed panic-free even for `Rupiah::MIN`.
    pub fn format_idr(&self) -> String {
        let val = self.0;
        if val == 0 {
            return "Rp 0".to_string();
        }

        let is_negative = val < 0;
        // Convert to u128 to handle i64::MIN without negation overflow
        let abs_val: u128 = val.unsigned_abs() as u128;
        let s = abs_val.to_string();

        let mut formatted = String::with_capacity(s.len() + s.len() / 3 + 4);
        let first_group_len = s.len() % 3;
        let mut idx = 0;

        if first_group_len > 0 {
            formatted.push_str(&s[0..first_group_len]);
            idx = first_group_len;
        }

        while idx < s.len() {
            if !formatted.is_empty() {
                formatted.push('.');
            }
            formatted.push_str(&s[idx..idx + 3]);
            idx += 3;
        }

        if is_negative {
            format!("-Rp {}", formatted)
        } else {
            format!("Rp {}", formatted)
        }
    }
}

// ---------------------------------------------------------------------------
// Standard Traits
// ---------------------------------------------------------------------------

impl fmt::Display for Rupiah {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_idr())
    }
}

impl From<i64> for Rupiah {
    #[inline]
    fn from(amount: i64) -> Self {
        Self(amount)
    }
}

impl From<Rupiah> for i64 {
    #[inline]
    fn from(rupiah: Rupiah) -> Self {
        rupiah.0
    }
}

impl Add for Rupiah {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        self.checked_add(rhs).expect("Rupiah addition overflow")
    }
}

impl Sub for Rupiah {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        self.checked_sub(rhs).expect("Rupiah subtraction overflow")
    }
}

impl Neg for Rupiah {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self::Output {
        self.checked_neg()
            .expect("Rupiah negation overflow on i64::MIN")
    }
}

impl Sum for Rupiah {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Rupiah::ZERO, |acc, x| {
            acc.checked_add(x)
                .expect("Rupiah overflow during iterator sum")
        })
    }
}

impl<'a> Sum<&'a Rupiah> for Rupiah {
    fn sum<I: Iterator<Item = &'a Rupiah>>(iter: I) -> Self {
        iter.fold(Rupiah::ZERO, |acc, x| {
            acc.checked_add(*x)
                .expect("Rupiah overflow during iterator sum")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_and_constants() {
        assert_eq!(Rupiah::ZERO.as_i64(), 0);
        assert_eq!(Rupiah::new(100_000).as_i64(), 100_000);
        assert_eq!(Rupiah::new(-50_000).into_inner(), -50_000);
        assert_eq!(Rupiah::MIN.as_i64(), i64::MIN);
        assert_eq!(Rupiah::MAX.as_i64(), i64::MAX);
    }

    #[test]
    fn test_predicates() {
        assert!(Rupiah::ZERO.is_zero());
        assert!(!Rupiah::ZERO.is_positive());
        assert!(!Rupiah::ZERO.is_negative());

        assert!(Rupiah::new(5_000).is_positive());
        assert!(!Rupiah::new(5_000).is_negative());
        assert!(!Rupiah::new(5_000).is_zero());

        assert!(Rupiah::new(-5_000).is_negative());
        assert!(!Rupiah::new(-5_000).is_positive());
        assert!(!Rupiah::new(-5_000).is_zero());
    }

    #[test]
    fn test_checked_add() {
        assert_eq!(
            Rupiah::new(50_000).checked_add(Rupiah::new(25_000)),
            Some(Rupiah::new(75_000))
        );
        assert_eq!(
            Rupiah::new(1_000_000).checked_add(Rupiah::ZERO),
            Some(Rupiah::new(1_000_000))
        );
        assert_eq!(
            Rupiah::new(50_000).checked_add(Rupiah::new(-20_000)),
            Some(Rupiah::new(30_000))
        );

        assert_eq!(Rupiah::MAX.checked_add(Rupiah::new(1)), None);
        assert_eq!(Rupiah::MAX.checked_add(Rupiah::MAX), None);
    }

    #[test]
    fn test_checked_sub() {
        assert_eq!(
            Rupiah::new(100_000).checked_sub(Rupiah::new(45_000)),
            Some(Rupiah::new(55_000))
        );
        assert_eq!(
            Rupiah::new(50_000).checked_sub(Rupiah::new(120_000)),
            Some(Rupiah::new(-70_000))
        );

        assert_eq!(Rupiah::MIN.checked_sub(Rupiah::new(1)), None);
    }

    #[test]
    fn test_checked_mul() {
        // Subscription calculation: 12 months @ Rp 5,000 = Rp 60,000
        assert_eq!(
            Rupiah::new(5_000).checked_mul(12),
            Some(Rupiah::new(60_000))
        );
        assert_eq!(Rupiah::new(1_000_000).checked_mul(0), Some(Rupiah::ZERO));
        assert_eq!(
            Rupiah::new(50_000).checked_mul(-1),
            Some(Rupiah::new(-50_000))
        );

        assert_eq!(Rupiah::MAX.checked_mul(2), None);
    }

    #[test]
    fn test_checked_div_and_rem() {
        assert_eq!(
            Rupiah::new(90_000).checked_div(3),
            Some(Rupiah::new(30_000))
        );
        assert_eq!(Rupiah::new(10_000).checked_div(3), Some(Rupiah::new(3_333)));

        let (q, r) = Rupiah::new(10_000).checked_div_rem(3).unwrap();
        assert_eq!(q, Rupiah::new(3_333));
        assert_eq!(r, Rupiah::new(1));
        assert_eq!(q.as_i64() * 3 + r.as_i64(), 10_000);

        assert_eq!(Rupiah::new(50_000).checked_div(0), None);
        assert_eq!(Rupiah::new(50_000).checked_div_rem(0), None);
        assert_eq!(Rupiah::MIN.checked_div(-1), None);
    }

    #[test]
    fn test_abs_and_neg() {
        assert_eq!(Rupiah::new(-50_000).abs(), Some(Rupiah::new(50_000)));
        assert_eq!(Rupiah::new(50_000).abs(), Some(Rupiah::new(50_000)));
        assert_eq!(Rupiah::MIN.abs(), None);

        assert_eq!(
            Rupiah::new(50_000).checked_neg(),
            Some(Rupiah::new(-50_000))
        );
        assert_eq!(
            Rupiah::new(-50_000).checked_neg(),
            Some(Rupiah::new(50_000))
        );
        assert_eq!(Rupiah::MIN.checked_neg(), None);
    }

    #[test]
    fn test_format_idr() {
        assert_eq!(Rupiah::new(0).format_idr(), "Rp 0");
        assert_eq!(Rupiah::new(500).format_idr(), "Rp 500");
        assert_eq!(Rupiah::new(5_000).format_idr(), "Rp 5.000");
        assert_eq!(Rupiah::new(150_000).format_idr(), "Rp 150.000");
        assert_eq!(Rupiah::new(1_000_000).format_idr(), "Rp 1.000.000");
        assert_eq!(
            Rupiah::new(12_345_678_901).format_idr(),
            "Rp 12.345.678.901"
        );

        assert_eq!(Rupiah::new(-5_000).format_idr(), "-Rp 5.000");
        assert_eq!(Rupiah::new(-1_500_000).format_idr(), "-Rp 1.500.000");

        assert_eq!(Rupiah::MAX.format_idr(), "Rp 9.223.372.036.854.775.807");
        assert_eq!(Rupiah::MIN.format_idr(), "-Rp 9.223.372.036.854.775.808");
    }

    #[test]
    fn test_serde_json_transparent_integer() {
        let val = Rupiah::new(50_000);
        let serialized = serde_json::to_string(&val).expect("Serialization failed");
        assert_eq!(serialized, "50000");

        let deserialized: Rupiah = serde_json::from_str("50000").expect("Deserialization failed");
        assert_eq!(deserialized, Rupiah::new(50_000));

        let negative_deserialized: Rupiah =
            serde_json::from_str("-75000").expect("Deserialization failed");
        assert_eq!(negative_deserialized, Rupiah::new(-75_000));

        let float_result: Result<Rupiah, _> = serde_json::from_str("50000.50");
        assert!(
            float_result.is_err(),
            "Serde must reject floating-point numbers"
        );
    }

    #[test]
    fn test_sum_aggregation() {
        let items = vec![Rupiah::new(10_000), Rupiah::new(25_000), Rupiah::new(5_000)];

        let sum: Rupiah = items.iter().sum();
        assert_eq!(sum, Rupiah::new(40_000));

        let checked = Rupiah::checked_sum(items);
        assert_eq!(checked, Some(Rupiah::new(40_000)));
    }

    #[test]
    fn test_basis_points_tax_calculation() {
        let transaction_amount = Rupiah::new(100_000);
        let tax = transaction_amount.checked_mul_bps(1_100);
        assert_eq!(tax, Some(Rupiah::new(11_000)));
    }
}
