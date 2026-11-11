# 02: TOML configuration and low-resource profile

**What to build:** A configuration file the user edits to shape the tool: general settings, sampling toggles, thresholds, alert settings and the names of protected processes. Built-in defaults apply when no file exists, and a low-resource profile shrinks the footprint for weak hardware.

**Blocked by:** 01: Cargo scaffold, CLI skeleton and core metric types.

**Status:** done

- [x] The tool loads a TOML config from a well-known user path and from a local file, and falls back to built-in defaults when absent
- [x] Sampling toggles, sampling interval, history duration, thresholds, alert settings and protected process names are all read from config
- [x] A `low-resource` profile lowers the sampling interval, shortens history and disables the expensive collectors
- [x] An invalid config fails with a message naming the offending key instead of panicking
- [x] Config parsing is covered by unit tests over fixture files
