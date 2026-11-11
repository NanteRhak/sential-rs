# 21: Spike detection against moving baselines

**What to build:** Spotting a sudden jump relative to normal behaviour, not only a fixed ceiling: a spike rule compares the current value against a baseline computed over a recent window.

**Blocked by:** 14: Threshold alert rules and terminal emission; 20: Ring buffer of high-resolution metrics.

**Status:** done

- [x] A rule can declare a spike as a multiple of a baseline computed over a recent window
- [x] Both a moving average and an exponentially weighted moving average are available as baselines
- [x] A spike rule also honours a minimum value, so a small fluctuation is not reported
- [x] A spike alert reports the current value and the baseline it was compared against
- [x] A test feeds a series with one large jump and asserts exactly one spike alert is raised
