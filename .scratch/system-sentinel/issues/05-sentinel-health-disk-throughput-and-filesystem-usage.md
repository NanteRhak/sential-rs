# 05: `sentinel health`: disk throughput and filesystem usage

**What to build:** The disk row of `sentinel health`: how fast the machine is reading and writing, which devices are busiest, and how full each filesystem is.

**Blocked by:** 04: `sentinel health`: CPU and memory summary.

**Status:** ready-for-agent

- [ ] The command gains a disk row with read and write bytes per second, read and write operations per second, a per-disk breakdown and filesystem usage percentage
- [ ] Read and write rates are differenced between ticks
- [ ] A device that disappears between ticks does not crash the command
- [ ] A filesystem that cannot be read is skipped with a note instead of failing the whole command
- [ ] A test asserts the disk row appears with the expected labels
