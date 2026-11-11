# 11: TUI: sorting, filtering and search

**What to build:** Finding a process in a live table: sort by any resource, narrow to one resource type, or search by name.

**Blocked by:** 10: TUI: system summary and live process table.

**Status:** ready-for-agent

- [ ] `t`, `m`, `i` and `N` sort the process table by CPU, memory, disk I/O and network respectively, and the active sort is visible
- [ ] `f` filters the table to a single resource type and `/` searches by process name, with a visible prompt while filtering
- [ ] Clearing a filter or a search restores the full list
- [ ] Filtering and searching happen over the live table without restarting collection
- [ ] A test drives the key handlers and asserts the visible rows change
