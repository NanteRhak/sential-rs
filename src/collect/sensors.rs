//! Hardware sensors: temperatures, fans, throttling and power profile.
//!
//! Everything here is optional. A machine that exposes none of it is normal,
//! not an error, so every reader returns what it found and nothing else.

use std::path::PathBuf;

/// What the machine reports about its own hardware.
#[derive(Debug, Clone, Default)]
pub struct SensorReadings {
    pub temperatures: Vec<(String, f64)>,
    pub fans: Vec<(String, u64)>,
    pub throttled: Option<bool>,
    pub power_profile: Option<String>,
}

/// The sysfs root. Defaults to `/sys`; tests point it at fixtures.
#[derive(Debug, Clone)]
pub struct SysFs {
    root: PathBuf,
}

impl Default for SysFs {
    fn default() -> Self {
        Self::system()
    }
}

impl SysFs {
    pub fn system() -> Self {
        Self {
            root: PathBuf::from("/sys"),
        }
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(path)).ok()
    }

    /// Everything the machine reports about its hardware.
    pub fn readings(&self) -> SensorReadings {
        SensorReadings {
            temperatures: self.temperatures(),
            fans: self.fans(),
            throttled: self.throttled(),
            power_profile: self.power_profile(),
        }
    }

    /// Every temperature the machine exposes.
    pub fn temperatures(&self) -> Vec<(String, f64)> {
        let mut readings = Vec::new();
        for zone in self.entries("class/thermal") {
            let Some(temp) = self.read(&format!("class/thermal/{zone}/temp")) else {
                continue;
            };
            let Ok(millidegrees) = temp.trim().parse::<f64>() else {
                continue;
            };
            let label = self
                .read(&format!("class/thermal/{zone}/type"))
                .map(|value| value.trim().to_string())
                .unwrap_or_else(|| zone.clone());
            let reading = (label, millidegrees / 1_000.0);
            if !readings.contains(&reading) {
                readings.push(reading);
            }
        }
        for hwmon in self.entries("class/hwmon") {
            for input in self.entries(&format!("class/hwmon/{hwmon}")) {
                if !input.starts_with("temp") || !input.ends_with("_input") {
                    continue;
                }
                let Some(temp) = self.read(&format!("class/hwmon/{hwmon}/{input}")) else {
                    continue;
                };
                let Ok(millidegrees) = temp.trim().parse::<f64>() else {
                    continue;
                };
                let base = input.trim_end_matches("_input");
                let label = self
                    .read(&format!("class/hwmon/{hwmon}/{base}_label"))
                    .map(|value| value.trim().to_string())
                    .unwrap_or_else(|| format!("{hwmon} {base}"));
                readings.push((label, millidegrees / 1_000.0));
            }
        }
        readings
    }

    /// Every fan speed the machine exposes, in revolutions per minute.
    pub fn fans(&self) -> Vec<(String, u64)> {
        let mut readings = Vec::new();
        for hwmon in self.entries("class/hwmon") {
            for input in self.entries(&format!("class/hwmon/{hwmon}")) {
                if !input.starts_with("fan") || !input.ends_with("_input") {
                    continue;
                }
                let Some(rpm) = self.read(&format!("class/hwmon/{hwmon}/{input}")) else {
                    continue;
                };
                let Ok(rpm) = rpm.trim().parse::<u64>() else {
                    continue;
                };
                let base = input.trim_end_matches("_input");
                let label = self
                    .read(&format!("class/hwmon/{hwmon}/{base}_label"))
                    .map(|value| value.trim().to_string())
                    .unwrap_or_else(|| format!("{hwmon} {base}"));
                readings.push((label, rpm));
            }
        }
        readings
    }

    /// Whether the processor is being throttled.
    ///
    /// `None` means the platform does not say either way.
    pub fn throttled(&self) -> Option<bool> {
        let mut found = false;
        for cpu in self.entries("devices/system/cpu") {
            if !cpu.starts_with("cpu") || !cpu.chars().skip(3).all(|c| c.is_ascii_digit()) {
                continue;
            }
            let dir = format!("devices/system/cpu/{cpu}/thermal_throttle");
            for count in self.entries(&dir) {
                if !count.ends_with("_throttle_count") {
                    continue;
                }
                let Some(value) = self.read(&format!("{dir}/{count}")) else {
                    continue;
                };
                let Ok(value) = value.trim().parse::<u64>() else {
                    continue;
                };
                found = true;
                if value > 0 {
                    return Some(true);
                }
            }
        }
        found.then_some(false)
    }

    /// The active power profile, when the platform has one.
    pub fn power_profile(&self) -> Option<String> {
        self.read("firmware/acpi/platform_profile")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    /// The entries of a directory, sorted so the readings come out in a stable
    /// order. A directory that cannot be read is simply empty.
    fn entries(&self, path: &str) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.root.join(path)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn a_machine_with_no_sensors_reports_nothing() {
        let sys = SysFs::new("/definitely/not/a/sysfs");

        assert!(sys.temperatures().is_empty());
        assert!(sys.fans().is_empty());
        assert_eq!(sys.throttled(), None);
        assert_eq!(sys.power_profile(), None);
    }

    #[test]
    fn a_broken_sensor_file_is_skipped() {
        let sys = SysFs::new(fixture("sys-empty"));

        assert!(sys.temperatures().is_empty());
    }

    #[test]
    fn a_sensor_reading_that_is_not_a_number_is_skipped() {
        let sys = SysFs::new(fixture("sys-broken"));

        assert!(sys.temperatures().is_empty());
    }

    #[test]
    fn thermal_zones_are_read_with_their_type_as_the_label() {
        let sys = SysFs::new(fixture("sys-thermal"));

        let readings = sys.temperatures();

        assert_eq!(readings.len(), 1, "{readings:?}");
        assert_eq!(readings[0].0, "x86_pkg_temp");
        assert!((readings[0].1 - 47.5).abs() < 0.001, "{readings:?}");
    }
}
