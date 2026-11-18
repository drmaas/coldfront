// SPDX-License-Identifier: GPL-3.0-only

/// A US ZIP code is five digits. Leading zeros count.
#[must_use]
pub fn is_us_zip(value: &str) -> bool {
    value.len() == 5 && value.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::is_us_zip;

    #[test]
    fn accepts_five_digits_including_leading_zero() {
        assert!(is_us_zip("00501"));
        assert!(!is_us_zip("9411"));
        assert!(!is_us_zip("94110-1234"));
    }
}
