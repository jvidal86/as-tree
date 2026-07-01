# Thin convenience wrappers around Cargo, for people who expect
#   make && make install
# to work. Cargo is the source of truth; see README.md for details.

.PHONY: build test lint audit install

build:
	cargo build --release

test:
	cargo test

lint:
	cargo clippy --all-targets

audit:
	cargo audit

install:
	cargo install --path .
