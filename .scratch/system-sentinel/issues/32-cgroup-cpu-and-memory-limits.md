# 32: cgroup CPU and memory limits

**What to build:** Cap what a runaway process can take — the last piece of resource management, turning "this process is hurting me" into "this process can have at most this much".

**Blocked by:** 17: Manual signals, suspend, resume and renice.

**Status:** ready-for-agent

- [ ] A process can be moved into a cgroup that caps its CPU and memory use
- [ ] Creating the cgroup and applying the limits is reported clearly, including when it needs privileges the user does not have
- [ ] Removing the limits returns the process to normal
- [ ] Protected processes are refused
- [ ] The action is written to the action log and supports dry-run
- [ ] A test asserts a limit is applied and then released
