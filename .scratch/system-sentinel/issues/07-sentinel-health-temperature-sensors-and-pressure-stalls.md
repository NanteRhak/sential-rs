# 07: `sentinel health`: temperature, sensors and pressure stalls

**What to build:** The hardware-health row of `sentinel health`: how hot the machine is, whether the fan is spinning, whether the CPU is being throttled, and how much time the kernel is losing to pressure stalls.

**Blocked by:** 04: `sentinel health`: CPU and memory summary.

**Status:** done

- [x] The command gains a temperature row showing CPU temperature plus any other temperature, fan speed and throttling sensors that can be read
- [x] The sensor collector degrades gracefully: when no sensor is readable the row says so instead of failing
- [x] Pressure stall information for CPU, memory and I/O is shown when the kernel exposes it and omitted when it does not
- [x] Broken or unreadable sensor files are skipped without aborting collection
- [x] A test asserts the command still succeeds on a machine with no readable sensor
