# 25: Policy model, matching and dry-run

**What to build:** The tool can decide what to do about a problem and show what it would do without doing it: rules that match conditions to proposed actions, dry by default.

**Blocked by:** 14: Threshold alert rules and terminal emission.

**Status:** done

- [x] Policies are read from config: name, condition, action, cooldown, confirmation requirement and excluded process names
- [x] A policy matches an alert or a metric condition and resolves to a proposed action
- [x] Every policy is disabled by default and never fires unless explicitly enabled
- [x] Running with dry-run enabled lists the actions that would have been taken, with the reason, and takes none of them
- [x] A test asserts a disabled policy never matches and an enabled dry-run policy proposes an action without performing it
