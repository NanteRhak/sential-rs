# Build, install and package Rust System Sentinel.

PREFIX ?= /usr/local
DESTDIR ?=
VERSION := $(shell cargo metadata --no-deps --format-version 1 | sed -n 's/.*"version":"\([^"]*\)".*/\1/p' | head -1)
BUILD := build
RELEASE := release
DEST := $(DESTDIR)$(PREFIX)

.PHONY: all build release test lint fmt install uninstall package clean

all: build

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

lint:
	cargo clippy --all-targets -- -D warnings
	cargo fmt --check

fmt:
	cargo fmt

install: release
	install -Dm755 target/$(RELEASE)/sentinel $(DEST)/bin/sentinel
	install -Dm644 packaging/systemd/sentinel.service $(DEST)/lib/systemd/system/sentinel.service
	install -Dm644 README.md $(DEST)/share/doc/sentinel/README.md

uninstall:
	rm -f $(DEST)/bin/sentinel
	rm -f $(DEST)/lib/systemd/system/sentinel.service
	rm -rf $(DEST)/share/doc/sentinel

# A release tarball with the binary, the service unit and the documentation.
package: release
	@mkdir -p $(BUILD)/sentinel-$(VERSION)
	install -Dm755 target/$(RELEASE)/sentinel $(BUILD)/sentinel-$(VERSION)/sentinel
	install -Dm644 packaging/systemd/sentinel.service $(BUILD)/sentinel-$(VERSION)/sentinel.service
	install -Dm644 README.md $(BUILD)/sentinel-$(VERSION)/README.md
	cp LICENSE $(BUILD)/sentinel-$(VERSION)/LICENSE 2>/dev/null || true
	tar -C $(BUILD) -czf $(BUILD)/sentinel-$(VERSION).tar.gz sentinel-$(VERSION)
	@echo "built $(BUILD)/sentinel-$(VERSION).tar.gz"

clean:
	cargo clean
	rm -rf $(BUILD)
