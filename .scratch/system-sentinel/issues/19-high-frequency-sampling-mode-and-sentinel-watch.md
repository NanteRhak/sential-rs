# 19: High-frequency sampling mode and `sentinel watch`

**What to build:** Catching what lasts only milliseconds: a fast sampling mode down to 100 ms, and a live watch view that updates at the requested rate.

**Blocked by:** 03: Sampling loop with delta and rate computation.

**Status:** done

- [x] The sampling interval can be set to 100 ms or lower, from config and from the command line
- [x] `sentinel watch --interval 100ms` shows the metrics updating live at the requested rate
- [x] Fast mode is available as a bounded burst session and can be stopped without leaving the tool in a bad state
- [x] The tool reports its own sampling rate so the user can confirm fast mode is actually running
- [x] A test asserts the sampler honours a 100 ms interval
