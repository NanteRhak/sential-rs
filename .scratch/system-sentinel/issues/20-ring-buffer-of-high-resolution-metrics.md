# 20: Ring buffer of high-resolution metrics

**What to build:** A bounded memory of the last seconds at high resolution, so a spike that lasted 200 ms is still there when the user goes looking for it.

**Blocked by:** 19: High-frequency sampling mode and `sentinel watch`.

**Status:** done

- [x] Recent high-resolution samples are kept in a bounded ring buffer
- [x] The buffer's size is bounded by the configured history duration, so memory use stays flat
- [x] Reading history never blocks the sampling loop
- [x] When the buffer overflows it keeps the newest samples and discards the oldest
- [x] A test fills the buffer past its bound and asserts the oldest samples are gone and the newest are retained
