# Installation Guide

This guide covers all installation methods for CCPM across different platforms.

## Requirements

- **Git 2.0 or later** (required for repository operations)
- **Rust 1.70 or later** (only for building from source)
- **Platform Support:**
  - Windows 10/11 (x86_64)
  - macOS 10.15+ (x86_64, aarch64)
  - Linux (x86_64, aarch64) - glibc 2.17+

## Quick Install

### Via Cargo (All Platforms)

If you have Rust installed, this is the easiest method:

```bash
# Install from crates.io (published via automated releases)
cargo install ccpm

# Install from GitHub (latest development)
cargo install --git https://github.com/aig787/ccpm.git
```

### Pre-built Binaries

CCPM provides automated releases with pre-built binaries for all major platforms:

```bash
# macOS (Intel)
curl -L https://github.com/aig787/ccpm/releases/latest/download/ccpm-x86_64-macos.tar.gz | tar xz

# macOS (Apple Silicon)
curl -L https://github.com/aig787/ccpm/releases/latest/download/ccpm-aarch64-macos.tar.gz | tar xz

# Linux (x86_64)
curl -L https://github.com/aig787/ccpm/releases/latest/download/ccpm-x86_64-linux.tar.gz | tar xz

# Linux (ARM64)
curl -L https://github.com/aig787/ccpm/releases/latest/download/ccpm-aarch64-linux.tar.gz | tar xz

# Windows (x86_64)
# Download: https://github.com/aig787/ccpm/releases/latest/download/ccpm-x86_64-windows.zip
```

## Platform-Specific Installation

### macOS

#### Manual Installation
```bash
# Download and install (automatically detects architecture)
curl -L https://github.com/aig787/ccpm/releases/latest/download/ccpm-$(uname -m)-macos.tar.gz | tar xz
sudo mv ccpm /usr/local/bin/
```

### Linux

#### Manual Installation
```bash
# Download and install (automatically detects architecture)
curl -L https://github.com/aig787/ccpm/releases/latest/download/ccpm-$(uname -m)-linux.tar.gz | tar xz
sudo mv ccpm /usr/local/bin/
```

### Windows

#### Manual Installation

```powershell
# Download and extract
Invoke-WebRequest -Uri "https://github.com/aig787/ccpm/releases/latest/download/ccpm-x86_64-windows.zip" -OutFile ccpm.zip
Expand-Archive -Path ccpm.zip -DestinationPath .

# Option 1: Install to System32 (requires admin)
Copy-Item ccpm.exe -Destination C:\Windows\System32\

# Option 2: Install to user directory (recommended)
New-Item -ItemType Directory -Force -Path "$env:LOCALAPPDATA\ccpm\bin"
Copy-Item ccpm.exe -Destination "$env:LOCALAPPDATA\ccpm\bin\"

# Add to PATH (user-level)
[Environment]::SetEnvironmentVariable("Path", $env:Path + ";$env:LOCALAPPDATA\ccpm\bin", [EnvironmentVariableTarget]::User)

# Restart PowerShell or refresh PATH
$env:Path = [System.Environment]::GetEnvironmentVariable("Path","User")
```

## Building from Source

### Prerequisites

- Rust 1.70 or later
- Git 2.0 or later
- Platform-specific requirements:
  - **Windows**: MSVC Build Tools or Visual Studio
  - **macOS**: Xcode Command Line Tools
  - **Linux**: gcc or clang, pkg-config

### Build Instructions

```bash
# Clone the repository
git clone https://github.com/aig787/ccpm.git
cd ccpm

# Build in release mode
cargo build --release

# Run tests
cargo test

# Install locally
cargo install --path .
```

### Platform-Specific Build

#### Unix/macOS
```bash
git clone https://github.com/aig787/ccpm.git
cd ccpm
cargo build --release
sudo cp target/release/ccpm /usr/local/bin/
```

#### Windows (PowerShell)
```powershell
git clone https://github.com/aig787/ccpm.git
cd ccpm
cargo build --release

# Install to user directory
New-Item -ItemType Directory -Force -Path "$env:LOCALAPPDATA\ccpm\bin"
Copy-Item target\release\ccpm.exe -Destination "$env:LOCALAPPDATA\ccpm\bin\"
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

After installation, verify CCPM is working:

```bash
# Check version
ccpm --version

# Show help
ccpm --help

# Initialize a test project
ccpm init
```

## Updating CCPM

### Via Cargo
```bash
cargo install --git https://github.com/aig787/ccpm.git --force
```

### Manual Update
Download the latest release and replace the existing binary.

## Uninstalling

### Via Cargo
```bash
cargo uninstall ccpm
```

### Manual Uninstall
```bash
# Unix/macOS
sudo rm /usr/local/bin/ccpm

# Windows
Remove-Item "$env:LOCALAPPDATA\ccpm\bin\ccpm.exe"
```

## Troubleshooting Installation

### Common Issues

#### Command Not Found

**Unix/macOS:**
```bash
# Check if ccpm is in PATH
which ccpm

# Add to PATH in ~/.bashrc or ~/.zshrc
export PATH="$PATH:/path/to/ccpm"
```

**Windows:**
```powershell
# Check if ccpm is in PATH
where.exe ccpm

# View current PATH
$env:Path -split ';'

# Add to PATH (see Manual Installation section)
```

#### Permission Denied

**Unix/macOS:**
```bash
# Make executable
chmod +x ccpm

# Use sudo for system directories
sudo cp ccpm /usr/local/bin/
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

CCPM handles long paths automatically, but you may need to enable system support:

```powershell
# Enable long paths (requires admin)
New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" `
    -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force
```

### Antivirus Software

Some antivirus software may flag or slow down CCPM operations. Consider:
- Adding CCPM to your antivirus exclusion list
- Excluding `~/.ccpm/cache/` directory from real-time scanning

## Next Steps

- Read the [User Guide](user-guide.md) to get started
- See [Command Reference](command-reference.md) for all commands
- Check [FAQ](faq.md) for common questions