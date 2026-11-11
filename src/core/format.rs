//! Unit conversion and human-readable formatting shared by every view.

/// `/proc/meminfo` and friends report kilobytes.
pub fn kb_to_bytes(kb: u64) -> u64 {
    kb * 1024
}

const UNITS: [(u64, &str); 4] = [
    (1024_u64.pow(4), "TiB"),
    (1024_u64.pow(3), "GiB"),
    (1024_u64.pow(2), "MiB"),
    (1024, "KiB"),
];

/// One number as a share of another, as a percentage.
pub fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64 * 100.0
    }
}

pub fn bytes_to_human(bytes: u64) -> String {
    for (scale, unit) in UNITS {
        if bytes >= scale {
            return format!("{:.1} {unit}", bytes as f64 / scale as f64);
        }
    }
    format!("{bytes} B")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kb_converts_to_bytes() {
        assert_eq!(kb_to_bytes(0), 0);
        assert_eq!(kb_to_bytes(1), 1024);
        assert_eq!(kb_to_bytes(4096), 4_194_304);
    }

    #[test]
    fn a_share_of_nothing_is_nothing() {
        assert_eq!(percent(10, 0), 0.0);
        assert_eq!(percent(25, 100), 25.0);
    }

    #[test]
    fn small_byte_counts_stay_exact() {
        assert_eq!(bytes_to_human(0), "0 B");
        assert_eq!(bytes_to_human(1023), "1023 B");
    }

    #[test]
    fn large_byte_counts_are_scaled_with_one_decimal() {
        assert_eq!(bytes_to_human(1024), "1.0 KiB");
        assert_eq!(bytes_to_human(1536), "1.5 KiB");
        assert_eq!(bytes_to_human(5 * 1024 * 1024), "5.0 MiB");
        assert_eq!(bytes_to_human(1024 * 1024 * 1024), "1.0 GiB");
        assert_eq!(bytes_to_human(1024_u64.pow(4)), "1.0 TiB");
    }
}
