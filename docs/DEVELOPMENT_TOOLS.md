# Development Tools

This document describes the recommended development tools for working on WhiteNoise.

## Required Tools

These tools are used by the project's scripts and CI:

- **rustc 1.90.0**: Rust compiler (specified in `rust-toolchain.toml`)
- **cargo**: Rust package manager (comes with rustc)
- **rustfmt**: Code formatter (specified in `rust-toolchain.toml`)
- **clippy**: Linter (specified in `rust-toolchain.toml`)

## Recommended Additional Tools

These tools enhance the development experience and are highly recommended:

### Installation

Run this command to install all recommended tools:

```bash
just install-tools
```

Or install them individually:

```bash
# Faster test runner with better output
cargo install cargo-nextest

# Security vulnerability scanner
cargo install cargo-audit

# Check for outdated dependencies
cargo install cargo-outdated

# Code coverage reporting
cargo install cargo-llvm-cov

# Check minimum supported Rust version
cargo install cargo-msrv

# Dependency license and vulnerability checking
cargo install cargo-deny

# Expand macros and see generated code
cargo install cargo-expand
```

## Tool Usage

### cargo-nextest
Faster parallel test execution with better output:

```bash
just test                # Automatically uses nextest if installed
just test-nextest        # Force nextest (requires installation)
just test-cargo          # Force standard cargo test
```

Note: The default `just test` command will automatically use nextest if it's installed, providing faster parallel test execution. If nextest is not installed, it will fall back to standard `cargo test`.

### cargo-audit
Check dependencies for known security vulnerabilities:

```bash
just audit               # Run security audit
```

### cargo-outdated
Find outdated dependencies:

```bash
just outdated            # Check for outdated deps
```

### cargo-llvm-cov
Generate code coverage reports:

```bash
just coverage            # Generate coverage report
just coverage-html       # Generate HTML coverage report
```

### cargo-msrv
Verify the minimum supported Rust version:

```bash
just check-msrv          # Verify MSRV is correct
```

### cargo-deny
Check dependencies for security issues, license compliance, and duplicates:

```bash
just deny-check          # Run all cargo-deny checks
```

### cargo-expand
See the expanded output of macros:

```bash
cargo expand             # Expand all modules
cargo expand module_name # Expand specific module
```

## Configuration Files

- **`deny.toml`**: Configuration for cargo-deny (license checking, security, etc.)
- **`clippy.toml`**: Additional clippy lint configuration
- **`.cargo/config.toml`**: Cargo build configuration (optional optimizations)

## CI Integration

The CI pipeline uses many of these tools:
- `cargo fmt` for format checking
- `cargo clippy` for linting
- `cargo audit` (via GitHub Action) for security scanning
- `cargo test` for running tests

Local pre-commit checks can be run with:

```bash
just precommit           # Full pre-commit check (includes integration tests)
just precommit-quick     # Quick check (skips integration tests)
```

