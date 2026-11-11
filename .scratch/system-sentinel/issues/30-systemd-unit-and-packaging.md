# 30: systemd unit and packaging

**What to build:** The tool installs cleanly on a Linux box: a systemd service that runs the daemon unattended, and a release artefact users can install.

**Blocked by:** 29: Daemon mode.

**Status:** ready-for-agent

- [ ] A systemd service unit is provided that starts the daemon, sets its log destination and restarts it on failure
- [ ] The service runs as an unprivileged user by default
- [ ] Release artefacts can be built as a plain binary and as a distribution package
- [ ] The README explains installation, configuration and the systemd unit
- [ ] A test verifies the service unit is valid and the binary builds in release mode
