# 10: TUI: system summary and live process table

**What to build:** `sentinel tui` opens a live dashboard: a system summary pane on top, a process table below, refreshing once per second.

**Blocked by:** 04: `sentinel health`: CPU and memory summary; 09: `sentinel ps`: per-process CPU percentage, sorting and JSON output.

**Status:** done

- [x] The TUI shows a system summary pane (CPU, memory, disk, network, temperature) and a process table pane, refreshing once per second
- [x] The TUI never blocks on collection: a slow or missing metric does not freeze the display
- [x] `q` quits cleanly and restores the terminal
- [x] The terminal is restored even when the TUI exits because of an error
- [x] A test renders the TUI into a buffer and asserts both panes are present
