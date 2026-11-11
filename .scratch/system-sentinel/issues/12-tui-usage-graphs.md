# 12: TUI: usage graphs

**What to build:** The graph pane of the TUI: CPU, memory, disk I/O and network drawn as live graphs, so the user sees shape rather than only current numbers.

**Blocked by:** 10: TUI: system summary and live process table.

**Status:** done

- [x] The TUI shows CPU, memory, disk I/O and network graphs drawn from recent samples, updating live
- [x] Temperature appears in the summary when a sensor is readable
- [x] Graphs draw from a bounded recent-history buffer so memory use stays flat over a long session
- [x] The graph pane degrades to a "no data" message rather than drawing nonsense before enough samples exist
- [x] A test renders the graph pane and asserts it produces output
