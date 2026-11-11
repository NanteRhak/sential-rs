# 01: Cargo scaffold, CLI skeleton and core metric types

**What to build:** A compiling `sentinel` binary that starts instantly, prints help, and holds the core metric model every later ticket fills in: SystemSample, CpuSample, MemorySample, DiskSample, NetworkSample, SensorSample, ProcessSample, ProcessState and Severity. The model lives behind a boundary that keeps operating-system collection separate from the values themselves.

**Blocked by:** None (can start immediately).

**Status:** done

- [x] `sentinel --help` and `sentinel --version` work and list the subcommands the tool will grow into
- [x] The core sample types carry the fields the specification's data model defines
- [x] Unit tests cover the pure logic of the core types, including unit conversion and value formatting
- [x] `cargo build`, `cargo test` and `cargo clippy` are clean
- [x] Startup does no work before a command is dispatched: no file, process or network access
