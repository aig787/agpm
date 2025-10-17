# Installation Guide

This guide covers all installation methods for AGPM across different platforms.

## Requirements

- **Git 2.5 or later** (required for worktree support and repository operations)
- **Rust 1.70 or later** (only for building from source)
- **Platform Support:**
  - Windows 10/11 (x86_64) - PowerShell 5.0+
  - macOS 10.15+ (x86_64, aarch64) - supports both Intel and Apple Silicon
  - Linux (x86_64, aarch64) - glibc 2.17+ or musl

## Quick Install

### Via Homebrew (macOS and Linux)

Homebrew provides automatic platform detection and easy updates:

```bash
# Install (automatically detects Apple Silicon/Intel/Linux ARM/x86_64)
brew install aig787/homebrew-agpm/agpm-cli

# Update to latest version
brew upgrade agpm-cli

# Uninstall
brew uninstall agpm-cli
```

**Requirements**: Homebrew 2.0+ (macOS or Linux)

### Via Cargo (All Platforms)

If you have Rust installed, this is a convenient method:

```bash
# Install latest stable version from crates.io
cargo install agpm-cli

# Install latest development version from GitHub
cargo install --git https://github.com/aig787/agpm.git

# Install specific version
cargo install agpm-cli --version 0.3.0
```

### Installer Scripts

Automated installer scripts for systems without Homebrew or Cargo:

**Unix/Linux/macOS:**
```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/aig787/agpm/releases/latest/download/agpm-installer.sh | sh
```

**Windows (PowerShell):**
```powershell
irm https://github.com/aig787/agpm/releases/latest/download/agpm-installer.ps1 | iex
```

### Manual Download

Download and install pre-built binaries directly from GitHub releases:

#### macOS (Apple Silicon)
```bash
mkdir -p ~/.agpm/bin
curl -L https://github.com/aig787/agpm/releases/latest/download/agpm-aarch64-apple-darwin.tar.xz | tar xJ -C ~/.agpm/bin
echo 'export PATH="$HOME/.agpm/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

#### macOS (Intel)
```bash
mkdir -p ~/.agpm/bin
curl -L https://github.com/aig787/agpm/releases/latest/download/agpm-x86_64-apple-darwin.tar.xz | tar xJ -C ~/.agpm/bin
echo 'export PATH="$HOME/.agpm/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

#### Linux (x86_64)
```bash
mkdir -p ~/.agpm/bin
curl -L https://github.com/aig787/agpm/releases/latest/download/agpm-x86_64-unknown-linux-gnu.tar.xz | tar xJ -C ~/.agpm/bin
echo 'export PATH="$HOME/.agpm/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

#### Linux (ARM64/aarch64)
```bash
mkdir -p ~/.agpm/bin
curl -L https://github.com/aig787/agpm/releases/latest/download/agpm-aarch64-unknown-linux-gnu.tar.xz | tar xJ -C ~/.agpm/bin
echo 'export PATH="$HOME/.agpm/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

#### Windows (PowerShell)
```powershell
# Download and extract to a user directory
$installPath = "$env:USERPROFILE\.agpm\bin"
New-Item -ItemType Directory -Force -Path $installPath
Invoke-WebRequest https://github.com/aig787/agpm/releases/latest/download/agpm-x86_64-pc-windows-msvc.zip -OutFile agpm.zip
Expand-Archive agpm.zip -DestinationPath $installPath -Force
Remove-Item agpm.zip

# Add to PATH for current session
$env:PATH += ";$installPath"

# Add to PATH permanently (user-level)
[Environment]::SetEnvironmentVariable("PATH", $env:PATH, [EnvironmentVariableTarget]::User)
```

## Platform-Specific Installation

### macOS

#### Manual Installation
```bash
# Download and install (automatically detects architecture)
curl -L https://github.com/aig787/agpm/releases/latest/download/agpm-$(uname -m)-macos.tar.gz | tar xz
sudo mv agpm /usr/local/bin/
```

### Linux

#### Manual Installation
```bash
# Download and install (automatically detects architecture)
curl -L https://github.com/aig787/agpm/releases/latest/download/agpm-$(uname -m)-linux.tar.gz | tar xz
sudo mv agpm /usr/local/bin/
```

### Windows

#### Manual Installation

```powershell
# Download and extract
Invoke-WebRequest -Uri "https://github.com/aig787/agpm/releases/latest/download/agpm-x86_64-windows.zip" -OutFile agpm.zip
Expand-Archive -Path agpm.zip -DestinationPath .

# Option 1: Install to System32 (requires admin)
Copy-Item agpm.exe -Destination C:\Windows\System32\

# Option 2: Install to user directory (recommended)
New-Item -ItemType Directory -Force -Path "$env:LOCALAPPDATA\agpm\bin"
Copy-Item agpm.exe -Destination "$env:LOCALAPPDATA\agpm\bin\"

# Add to PATH (user-level)
[Environment]::SetEnvironmentVariable("Path", $env:Path + ";$env:LOCALAPPDATA\agpm\bin", [EnvironmentVariableTarget]::User)

# Restart PowerShell or refresh PATH
$env:Path = [System.Environment]::GetEnvironmentVariable("Path","User")
```

## Building from Source

### Prerequisites

- Rust 1.70 or later
- Git 2.0 or later
- Platform-specific requirements:
  - **Windows**: MSVC Build Tools or Visual Studio
  - **macOS**: Xcode Command Line Tools, tar (included in macOS)
  - **Linux**: gcc or clang, pkg-config, tar with xz support (usually pre-installed)

### Build Instructions

```bash
# Clone the repository
git clone https://github.com/aig787/agpm.git
cd agpm

# Build in release mode with optimizations
cargo build --release

# Run the full test suite (uses cargo nextest for faster parallel testing)
cargo nextest run
cargo test --doc

# Check code formatting and linting
cargo fmt --check
cargo clippy -- -D warnings

# Install locally
cargo install --path .
```

### Platform-Specific Build

#### Unix/macOS
```bash
git clone https://github.com/aig787/agpm.git
cd agpm
cargo build --release
sudo cp target/release/agpm /usr/local/bin/
```

#### Windows (PowerShell)
```powershell
git clone https://github.com/aig787/agpm.git
cd agpm
cargo build --release

# Install to user directory
New-Item -ItemType Directory -Force -Path "$env:LOCALAPPDATA\agpm\bin"
Copy-Item target\release\agpm.exe -Destination "$env:LOCALAPPDATA\agpm\bin\"
```

### Cross-Compilation

```bash
# Add target platforms
rustup target add x86_64-apple-darwin
rustup target add x86_64-unknown-linux-gnu
rustup target add x86_64-pc-windows-msvc
rustup target add aarch64-apple-darwin
rustup target add aarch64-unknown-linux-gnu

# Build for specific targets
cargo build --target x86_64-apple-darwin --release
cargo build --target x86_64-unknown-linux-gnu --release
cargo build --target x86_64-pc-windows-msvc --release
cargo build --target aarch64-apple-darwin --release
cargo build --target aarch64-unknown-linux-gnu --release
```

## Verifying Installation

After installation, verify AGPM is working:

```bash
# Check version and verify installation
agpm --version

# Show help and available commands
agpm --help

# Test Git worktree support (requires Git 2.5+)
git --version

# Initialize a test project
agpm init

# Test parallel installation capabilities
agpm install --help | grep max-parallel
```

## Updating AGPM

### Via Cargo
```bash
# Update to latest stable version
cargo install agpm-cli --force

# Update to latest development version
cargo install --git https://github.com/aig787/agpm.git --force
```

### Manual Update
Download the latest release and replace the existing binary.

## Uninstalling

### Via Cargo
```bash
cargo uninstall agpm-cli
```

### Manual Uninstall
```bash
# Unix/macOS
sudo rm /usr/local/bin/agpm

# Windows
Remove-Item "$env:LOCALAPPDATA\agpm\bin\agpm.exe"
```

## Troubleshooting Installation

### Common Issues

#### Command Not Found

**Unix/macOS:**
```bash
# Check if agpm is in PATH
which agpm

# Add to PATH in ~/.bashrc or ~/.zshrc
export PATH="$PATH:/path/to/agpm"
```

**Windows:**
```powershell
# Check if agpm is in PATH
where.exe agpm

# View current PATH
$env:Path -split ';'

# Add to PATH (see Manual Installation section)
```

#### Permission Denied

**Unix/macOS:**
```bash
# Make executable
chmod +x agpm

# Use sudo for system directories
sudo cp agpm /usr/local/bin/
```

**Windows:**
Run PowerShell as Administrator or install to user directory.

#### Build Failures

**Missing Rust:**
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Windows: Missing MSVC:**
Install Visual Studio Build Tools from https://visualstudio.microsoft.com/downloads/

#### Git Not Found

**Windows:**
```powershell
winget install Git.Git
# Or download from https://git-scm.com/download/win
```

**macOS:**
```bash
# Install via Xcode Command Line Tools
xcode-select --install
```

**Linux:**
```bash
# Debian/Ubuntu
sudo apt-get install git

# RHEL/CentOS
sudo yum install git
```

## Windows-Specific Notes

### Long Path Support

AGPM handles long paths automatically, but you may need to enable system support:

```powershell
# Enable long paths (requires admin)
New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" `
    -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force
```

### Antivirus Software

Some antivirus software may flag or slow down AGPM operations. Consider:
- Adding AGPM to your antivirus exclusion list
- Excluding `~/.agpm/cache/` directory from real-time scanning

## Next Steps

- Read the [User Guide](user-guide.md) to get started
- See [Command Reference](command-reference.md) for all commands
- Check [FAQ](faq.md) for common questions