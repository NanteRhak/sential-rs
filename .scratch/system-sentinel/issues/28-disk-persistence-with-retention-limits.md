# 28: Disk persistence with retention limits

**What to build:** History that survives a restart, on disk, without ever filling the disk.

**Blocked by:** 27: In-memory history and `sentinel report`.

**Status:** done

- [x] History can be persisted to disk so it survives a restart
- [x] Persistence is optional and off unless enabled in config
- [x] Retention limits stop the store growing without bound, and the oldest data is dropped first
- [x] A full or unwritable disk is reported and does not stop monitoring
- [x] Writes are batched rather than performed per sample, so persistence adds no per-tick cost
- [x] A test writes history, reloads it, and asserts the retention limit was applied
