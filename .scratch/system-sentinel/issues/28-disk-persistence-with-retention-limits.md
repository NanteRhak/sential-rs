# 28: Disk persistence with retention limits

**What to build:** History that survives a restart, on disk, without ever filling the disk.

**Blocked by:** 27: In-memory history and `sentinel report`.

**Status:** ready-for-agent

- [ ] History can be persisted to disk so it survives a restart
- [ ] Persistence is optional and off unless enabled in config
- [ ] Retention limits stop the store growing without bound, and the oldest data is dropped first
- [ ] A full or unwritable disk is reported and does not stop monitoring
- [ ] Writes are batched rather than performed per sample, so persistence adds no per-tick cost
- [ ] A test writes history, reloads it, and asserts the retention limit was applied
