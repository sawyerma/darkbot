//! Exact fixed-point values of the strategy's features: a rational is
//! rounded once, half to even, to `places` decimal places and kept as a
//! signed integer scaled by `10^places`. No floating point.

use alloy_primitives::{U256, U512};

/// `10^places` (`places` ≤ 38).
pub fn scale(places: u32) -> Option<i128> {
    10i128.checked_pow(places)
}

/// `num / den` rounded half to even at `places`; `None` for a zero
/// denominator or a value outside `i128`.
pub fn ratio(num: U256, den: U256, places: u32) -> Option<i128> {
    if den.is_zero() {
        return None;
    }
    let s = U512::from(u128::try_from(scale(places)?).ok()?);
    let n = U512::from(num) * s;
    let d = U512::from(den);
    let (q, r) = (n / d, n % d);
    let twice = r * U512::from(2u8);
    let q = if twice > d || (twice == d && q.bit(0)) {
        q + U512::from(1u8)
    } else {
        q
    };
    i128::try_from(q).ok()
}

/// A decimal string (`-12.5`, `0.25`, `3`) at exactly `places` places;
/// more digits than `places` are refused (no silent rounding of a bound).
pub fn parse(text: &str, places: u32) -> Option<i128> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
    if int.is_empty()
        || !int.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
        || frac.len() > places as usize
        || (digits.contains('.') && frac.is_empty())
    {
        return None;
    }
    let mut value: i128 = int.parse::<i128>().ok()?.checked_mul(scale(places)?)?;
    let pad = places as usize - frac.len();
    if !frac.is_empty() {
        let f: i128 = frac.parse().ok()?;
        value = value.checked_add(f.checked_mul(scale(pad as u32)?)?)?;
    }
    Some(if negative { -value } else { value })
}

/// The decimal text of a scaled value.
pub fn text(value: i128, places: u32) -> String {
    let s = scale(places).unwrap_or(1);
    let sign = if value < 0 { "-" } else { "" };
    let abs = value.unsigned_abs();
    let (int, frac) = (abs / s as u128, abs % s as u128);
    if places == 0 {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}.{frac:0width$}", width = places as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios_round_half_to_even_once() {
        let r = |n: u64, d: u64, p| ratio(U256::from(n), U256::from(d), p);
        assert_eq!(r(1, 3, 4), Some(3_333));
        assert_eq!(r(2, 3, 4), Some(6_667));
        // Exactly half: to the even neighbour.
        assert_eq!(r(1, 8, 2), Some(12));
        assert_eq!(r(3, 8, 2), Some(38));
        assert_eq!(r(5, 1, 0), Some(5));
        assert_eq!(r(1, 0, 4), None);
    }

    #[test]
    fn bounds_parse_exactly_or_not_at() {
        assert_eq!(parse("0.25", 4), Some(2_500));
        assert_eq!(parse("-1.5", 2), Some(-150));
        assert_eq!(parse("3", 0), Some(3));
        assert_eq!(parse("0.12345", 4), None);
        assert_eq!(parse("1.", 4), None);
        assert_eq!(parse(".5", 4), None);
        assert_eq!(parse("1e3", 4), None);
        assert_eq!(text(-150, 2), "-1.50");
        assert_eq!(text(2_500, 4), "0.2500");
        assert_eq!(text(7, 0), "7");
    }
}
