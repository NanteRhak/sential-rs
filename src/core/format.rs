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
