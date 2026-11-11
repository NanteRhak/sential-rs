# 06: `sentinel health`: network throughput and packet rate

**What to build:** The network row of `sentinel health`: how much data is moving in and out, at what packet rate, and how many errors and drops each interface is seeing.

**Blocked by:** 04: `sentinel health`: CPU and memory summary.

**Status:** done

- [x] The command gains a network row with bytes received and transmitted per second, packet rate, error count and drop count, broken down per interface
- [x] The loopback interface is excluded from the totals unless the user asks for it
- [x] Interfaces that appear or vanish between ticks are handled without crashing
- [x] Rates are differenced between ticks
- [x] A test asserts the network row appears with the expected labels
