.PHONY: build release test test-stress test-stress-verbose test-all clean fmt check coverage install help

# Default target
all: build

# Build in debug mode
build:
	cargo build

# Build in release mode
release:
	cargo build --release

# Run integration tests (fast, runs in CI)
test:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v cargo-nextest >/dev/null 2>&1 || { echo "Installing cargo-nextest..."; cargo binstall cargo-nextest --secure; }
	cargo nextest run --test integration
	cargo test --doc

# Run stress tests (slow, manual execution only)
test-stress:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v cargo-nextest >/dev/null 2>&1 || { echo "Installing cargo-nextest..."; cargo binstall cargo-nextest --secure; }
	@echo "Running stress tests (this may take several minutes)..."
	cargo nextest run --test stress

# Run stress tests with verbose output
test-stress-verbose:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v cargo-nextest >/dev/null 2>&1 || { echo "Installing cargo-nextest..."; cargo binstall cargo-nextest --secure; }
	@echo "Running stress tests with verbose output..."
	RUST_LOG=debug cargo test --test stress -- --nocapture

# Run all tests (integration + stress + doc tests)
test-all:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v cargo-nextest >/dev/null 2>&1 || { echo "Installing cargo-nextest..."; cargo binstall cargo-nextest --secure; }
	cargo nextest run --test integration
	cargo nextest run --test stress
	cargo test --doc

# Run tests with verbose output
test-verbose:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v cargo-nextest >/dev/null 2>&1 || { echo "Installing cargo-nextest..."; cargo binstall cargo-nextest --secure; }
	RUST_LOG=debug cargo nextest run --test integration --nocapture
	RUST_LOG=debug cargo test --doc -- --nocapture

# Clean build artifacts
clean:
	cargo clean

# Format code
fmt:
	cargo fmt

# Format check (CI-friendly)
fmt-check:
	cargo fmt -- --check

# Run clippy linter
lint:
	cargo clippy -- -D warnings

# Run all checks (fmt, clippy, test)
check: fmt-check lint test

# Run tests with coverage (requires cargo-llvm-cov)
coverage:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { echo "Installing cargo-llvm-cov..."; cargo binstall cargo-llvm-cov --secure; }
	cargo llvm-cov --ignore-filename-regex "test_utils" --html --output-dir target/coverage
	@echo ""
	@echo "Coverage report generated at: target/coverage/html/index.html"
	@cargo llvm-cov --ignore-filename-regex "test_utils" --summary-only

# Install the binary locally
install:
	cargo install --path .

# Run the binary
run:
	cargo run

# Run with debug logging
run-debug:
	RUST_LOG=debug cargo run

# Watch for changes and rebuild (requires cargo-watch)
watch:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v cargo-watch >/dev/null 2>&1 || { echo "Installing cargo-watch..."; cargo binstall cargo-watch --secure; }
	@command -v cargo-nextest >/dev/null 2>&1 || { echo "Installing cargo-nextest..."; cargo binstall cargo-nextest --secure; }
	cargo watch -x build -x "nextest run" -x "test --doc"

# Detect current platform
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

# Set platform-specific variables
ifeq ($(UNAME_S),Linux)
    CURRENT_PLATFORM := linux
    CROSS_TARGETS := x86_64-pc-windows-gnullvm x86_64-apple-darwin
    CROSS_NAMES := Windows macOS
endif
ifeq ($(UNAME_S),Darwin)
    CURRENT_PLATFORM := macos
    CROSS_TARGETS := x86_64-pc-windows-gnullvm x86_64-unknown-linux-gnu
    CROSS_NAMES := Windows Linux
endif
ifeq ($(OS),Windows_NT)
    CURRENT_PLATFORM := windows
    CROSS_TARGETS := x86_64-unknown-linux-gnu x86_64-apple-darwin
    CROSS_NAMES := Linux macOS
endif

# Cross-compilation setup
cross-setup:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v cargo-zigbuild >/dev/null 2>&1 || { echo "Installing cargo-zigbuild..."; cargo binstall cargo-zigbuild --secure; }
	@command -v zig >/dev/null 2>&1 || { echo "Please install Zig from https://ziglang.org/download/"; exit 1; }
	@echo "Ensuring Rust targets are installed..."
	@for target in $(CROSS_TARGETS); do \
		rustup target list --installed | grep -q "$$target" || { \
			echo "Installing target: $$target..."; \
			rustup target add $$target || exit 1; \
		}; \
	done

# Cross-compile for all platforms (current + other two)
cross-all: cross-setup
	@echo "Building for all platforms..."
	@echo "Current platform: $(CURRENT_PLATFORM)"
	@echo ""
	@echo "Building for current platform..."
	@cargo build --release
	@echo "  Built: target/release/agpm"
	@echo ""
	@echo "Cross-compiling for: $(CROSS_NAMES)"
	@for target in $(CROSS_TARGETS); do \
		echo "Building for $$target..."; \
		cargo zigbuild --release --target $$target || exit 1; \
		if echo $$target | grep -q windows; then \
			echo "  Built: target/$$target/release/agpm.exe"; \
		else \
			echo "  Built: target/$$target/release/agpm"; \
		fi; \
	done
	@echo ""
	@echo "All builds complete!"
	@echo "Binaries available:"
	@echo "  - $(CURRENT_PLATFORM): target/release/agpm"
	@for target in $(CROSS_TARGETS); do \
		if echo $$target | grep -q windows; then \
			echo "  - Windows: target/$$target/release/agpm.exe"; \
		elif echo $$target | grep -q linux; then \
			echo "  - Linux: target/$$target/release/agpm"; \
		elif echo $$target | grep -q darwin; then \
			echo "  - macOS: target/$$target/release/agpm"; \
		fi; \
	done

# Individual cross-compilation targets
cross-windows:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v cargo-zigbuild >/dev/null 2>&1 || { echo "Installing cargo-zigbuild..."; cargo binstall cargo-zigbuild --secure; }
	@command -v zig >/dev/null 2>&1 || { echo "Please install Zig from https://ziglang.org/download/"; exit 1; }
	@rustup target list --installed | grep -q "x86_64-pc-windows-gnullvm" || { \
		echo "Installing target: x86_64-pc-windows-gnullvm..."; \
		rustup target add x86_64-pc-windows-gnullvm || exit 1; \
	}
	cargo zigbuild --release --target x86_64-pc-windows-gnullvm
	@echo "Windows binary built at: target/x86_64-pc-windows-gnullvm/release/agpm.exe"

cross-linux:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v cargo-zigbuild >/dev/null 2>&1 || { echo "Installing cargo-zigbuild..."; cargo binstall cargo-zigbuild --secure; }
	@command -v zig >/dev/null 2>&1 || { echo "Please install Zig from https://ziglang.org/download/"; exit 1; }
	@rustup target list --installed | grep -q "x86_64-unknown-linux-gnu" || { \
		echo "Installing target: x86_64-unknown-linux-gnu..."; \
		rustup target add x86_64-unknown-linux-gnu || exit 1; \
	}
	cargo zigbuild --release --target x86_64-unknown-linux-gnu
	@echo "Linux binary built at: target/x86_64-unknown-linux-gnu/release/agpm"

cross-macos:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v cargo-zigbuild >/dev/null 2>&1 || { echo "Installing cargo-zigbuild..."; cargo binstall cargo-zigbuild --secure; }
	@command -v zig >/dev/null 2>&1 || { echo "Please install Zig from https://ziglang.org/download/"; exit 1; }
	@rustup target list --installed | grep -q "x86_64-apple-darwin" || { \
		echo "Installing target: x86_64-apple-darwin..."; \
		rustup target add x86_64-apple-darwin || exit 1; \
	}
	cargo zigbuild --release --target x86_64-apple-darwin
	@echo "macOS binary built at: target/x86_64-apple-darwin/release/agpm"

# Install cargo-dist for distribution tasks
dist-setup:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v dist >/dev/null 2>&1 || { echo "Installing cargo-dist..."; cargo binstall cargo-dist --secure; }

# Test cargo-dist configuration
dist-plan:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v dist >/dev/null 2>&1 || { echo "Installing cargo-dist..."; cargo binstall cargo-dist --secure; }
	dist plan

# Generate cargo-dist artifacts locally
dist-build:
	@command -v cargo-binstall >/dev/null 2>&1 || { echo "Installing cargo-binstall..."; cargo install cargo-binstall; }
	@command -v dist >/dev/null 2>&1 || { echo "Installing cargo-dist..."; cargo binstall cargo-dist --secure; }
	dist build

# Display help
help:
	@echo "AGPM Makefile Commands:"
	@echo ""
	@echo "Build:"
	@echo "  make build         - Build in debug mode"
	@echo "  make release       - Build in release mode"
	@echo "  make clean         - Clean build artifacts"
	@echo ""
	@echo "Testing:"
	@echo "  make test          - Run integration tests (fast, CI default)"
	@echo "  make test-stress   - Run stress tests (slow, manual only)"
	@echo "  make test-all      - Run all tests (integration + stress + doc)"
	@echo "  make test-verbose  - Run integration tests with verbose output"
	@echo "  make test-stress-verbose - Run stress tests with verbose output"
	@echo "  make coverage      - Run tests with coverage report"
	@echo ""
	@echo "Code Quality:"
	@echo "  make fmt           - Format code"
	@echo "  make lint          - Run clippy linter"
	@echo "  make check         - Run fmt-check, lint, and integration tests"
	@echo ""
	@echo "Development:"
	@echo "  make install       - Install binary locally"
	@echo "  make run           - Run the binary"
	@echo "  make watch         - Watch and rebuild on changes"
	@echo ""
	@echo "Cross-compilation:"
	@echo "  make cross-all     - Cross-compile for the other two platforms"
	@echo "  make cross-windows - Cross-compile for Windows"
	@echo "  make cross-linux   - Cross-compile for Linux"
	@echo "  make cross-macos   - Cross-compile for macOS"
	@echo ""
	@echo "Distribution:"
	@echo "  make dist-setup    - Install cargo-dist tool"
	@echo "  make dist-plan     - Test cargo-dist configuration"
	@echo "  make dist-build    - Generate cargo-dist artifacts locally"
	@echo ""
	@echo "  make help          - Show this help message"