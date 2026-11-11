# 26: Automatic actions and protected processes

**What to build:** Policies that really act, with the guardrails: critical processes stay untouched, everything is logged, and the user opted in explicitly.

**Blocked by:** 16: `sentinel kill`: safe manual termination with action log; 25: Policy model, matching and dry-run.

**Status:** ready-for-agent

- [ ] A policy whose condition is met performs its action on the matching process
- [ ] Protected and critical processes are never terminated by an automatic policy unless the policy explicitly allows it
- [ ] Protected processes are visibly marked in the process list and in the TUI
- [ ] A protected process whose own usage is abnormal still raises an alert
- [ ] Every automatic action is written to the action log with the policy that caused it and the outcome
- [ ] A test asserts an automatic kill policy refuses a protected process and permits an unprotected one
