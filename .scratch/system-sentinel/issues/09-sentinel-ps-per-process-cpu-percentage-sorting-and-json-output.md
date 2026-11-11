# 09: `sentinel ps`: per-process CPU percentage, sorting and JSON output

**What to build:** `sentinel ps` becomes the process list people actually use: a CPU percentage column next to the memory one, sorting by whichever resource the user cares about, and JSON for scripting.

**Blocked by:** 08: `sentinel ps`: process identity and memory listing.

**Status:** ready-for-agent

- [ ] The command adds a CPU percentage column computed from the difference between two ticks
- [ ] `--sort cpu|mem|name` orders the table by the chosen column, defaulting to CPU
- [ ] `--json` prints the same rows as machine-readable JSON for scripting
- [ ] `sentinel top` renders the same table once, sorted by CPU, then exits
- [ ] A process that appears after the first tick shows a zero CPU percentage rather than a spike
- [ ] A test asserts the JSON output parses and that the sort option reorders rows
