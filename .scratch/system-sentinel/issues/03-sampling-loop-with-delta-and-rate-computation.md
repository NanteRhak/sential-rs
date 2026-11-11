# 03: Sampling loop with delta and rate computation

**What to build:** The heartbeat behind everything: a sampling loop that ticks at the configured interval and turns raw counters into rates, so every later command sees consistent samples no matter which collector produced them.

**Blocked by:** 01: Cargo scaffold, CLI skeleton and core metric types; 02: TOML configuration and low-resource profile.

**Status:** ready-for-agent

- [ ] A sampling loop produces one SystemSample per tick at the configured interval, driven by a monotonic clock
- [ ] Counter-based metrics (bytes, jiffies) become per-second rates by differencing against the previous tick; the first tick reports zero rates rather than nonsense
- [ ] A tick that overruns its interval does not pile up or run concurrently
- [ ] Sampler state is readable by the UI without blocking collection
- [ ] Unit tests cover rate calculation, including the zero-delta and counter-reset cases
