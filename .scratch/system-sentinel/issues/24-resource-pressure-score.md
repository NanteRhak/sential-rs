# 24: Resource pressure score

**What to build:** One number per process that says how much damage it is doing to the system's responsiveness, so the worst offender is obvious even when it does not top any single column.

**Blocked by:** 09: `sentinel ps`: per-process CPU percentage, sorting and JSON output.

**Status:** done

- [x] Every process gets a pressure score combining CPU, memory, I/O, network, file descriptor and restart behaviour into one weighted value
- [x] The weights are configurable
- [x] Each component is normalised against the current system, so the score means the same thing on a small machine and a large one
- [x] The score is shown in the process list and can be used as a sort key
- [x] A test asserts the score rises for a process using more resources and that the weights actually change the result
