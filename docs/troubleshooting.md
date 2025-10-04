# Troubleshooting Guide

This guide covers common issues and their solutions.

## Installation Issues

### Command Not Found

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

# Add to PATH for current session
$env:Path += ";$env:LOCALAPPDATA\agpm\bin"

# Add to PATH permanently
[Environment]::SetEnvironmentVariable("Path", $env:Path + ";$env:LOCALAPPDATA\agpm\bin", [EnvironmentVariableTarget]::User)
```

### Permission Denied

**Unix/macOS:**
```bash
# Make executable
chmod +x agpm

# Use sudo for system directories
sudo cp agpm /usr/local/bin/
```

**Windows:**
Run PowerShell as Administrator or install to user directory instead of system directories.

### Build Failures

**Missing Rust:**
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Windows - Missing MSVC:**
Install Visual Studio Build Tools from https://visualstudio.microsoft.com/downloads/

## Runtime Issues

### Git Not Found

**Windows:**
```powershell
# Download from https://git-scm.com/download/win
```

**macOS:**
```bash
# Via Xcode Command Line Tools
xcode-select --install
```

**Linux:**
```bash
# Install git using your distribution's standard method
# Check your distribution's documentation for installation instructions
```

### No Manifest Found

```bash
# Initialize a new project
agpm init

# Or specify manifest path
agpm install --manifest-path ./path/to/agpm.toml
```

## Dependency Issues

### Version Conflicts

```bash
# Check for conflicts
agpm validate --resolve

# View detailed resolution
RUST_LOG=debug agpm validate --resolve
```

**Common solutions:**
- Widen version constraints in agpm.toml
- Use exact versions to pin dependencies
- Check if sources have compatible versions

### Lockfile Out of Sync

- Check the exact staleness reason with `agpm validate --check-lock`.
- Rerun `agpm install` to regenerate the lockfile; the resolver keeps existing versions unless the manifest or upstream reference changed.
- Remove `agpm.lock` only when you intentionally want a clean rebuild (e.g., recovering from manual edits or corruption).

```bash
# Regenerate lockfile in place
agpm install

# Last resort: rebuild from scratch
rm agpm.lock
agpm install
```

### Dependency Not Found

Check if the file exists in the source repository:

```bash
# List repository contents
git ls-tree -r HEAD --name-only https://github.com/org/repo.git

# Or clone and check manually
git clone https://github.com/org/repo.git /tmp/check
ls /tmp/check/agents/
```

## Authentication Issues

### Private Repository Access

**SSH Issues:**
```bash
# Test SSH connection
ssh -T git@github.com

# Check SSH key
ssh-add -l

# Add SSH key
ssh-add ~/.ssh/id_rsa
```

**HTTPS Token Issues:**
```bash
# Verify token in global config
agpm config show

# Test git access directly
git ls-remote https://token@github.com/org/repo.git
```

### Token Expired

```bash
# Update token
agpm config edit

# Or use command
agpm config add-source private "https://oauth2:NEW_TOKEN@github.com/org/repo.git"

# Clear cache and retry
agpm cache clean --all
agpm install --no-cache
```

## Cache Issues

### Corrupted Cache

```bash
# Clean specific source
agpm cache clean

# Clear entire cache (including worktrees)
agpm cache clean --all

# Bypass cache
agpm install --no-cache

# Clean only worktrees (keep bare repositories)
agpm cache clean --worktrees
```

### Disk Space

```bash
# Check cache size
agpm cache info

# Clean unused entries
agpm cache clean

# Change cache location
# Edit ~/.agpm/config.toml
[settings]
cache_dir = "/larger/disk/cache"
```

## Resource Issues

### Scripts Not Executing

**Check permissions:**
```bash
ls -la .claude/agpm/scripts/
chmod +x .claude/agpm/scripts/*.sh
```

**Check interpreter:**
```bash
# Verify shebang line
head -1 .claude/agpm/scripts/script.sh

# Check if interpreter exists
which bash
which python3
```

### Hooks Not Triggering

**Check configuration:**
```bash
# Verify hook is in settings
cat .claude/settings.local.json | grep "hook-name"

# Check hook file exists
ls .claude/agpm/hooks/
```

**Debug hooks:**
```json
// Add to hook configuration
{
  "debug": true,
  "verbose": true
}
```

### MCP Servers Not Starting

**Check runtime:**
```bash
# For npx-based servers
which npx
npm --version

# For Python servers
which uvx
python --version
```

**Check configuration:**
```bash
# Verify server in .mcp.json
cat .mcp.json | grep "server-name"

# Test command manually
npx -y @modelcontextprotocol/server-filesystem --help
```

## Platform-Specific Issues

### Windows Long Paths

```powershell
# Enable long path support (requires admin)
New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" `
    -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force

# Restart required
```

### Windows Line Endings

```bash
# Configure Git to handle line endings
git config --global core.autocrlf true

# Convert existing files
dos2unix .claude/agpm/scripts/*.sh
```

### macOS Gatekeeper

If macOS blocks the binary:

```bash
# Remove quarantine attribute
xattr -d com.apple.quarantine agpm

# Or allow in System Preferences > Security & Privacy
```

### Linux Permission Issues

```bash
# Fix ownership
sudo chown -R $USER:$USER ~/.agpm

# Fix permissions
chmod -R u+rw ~/.agpm
find ~/.agpm -type d -exec chmod u+x {} \;
```

## Worktree Issues

### Worktree Creation Failures

**Concurrent access conflicts:**
```bash
# Check for existing worktrees
ls ~/.agpm/cache/worktrees/

# Clean stale worktrees
agpm cache clean --worktrees

# Retry with fresh worktrees
agpm install --no-cache
```

**Bare repository issues:**
```bash
# Verify bare repository exists
ls ~/.agpm/cache/sources/

# Check if bare repo has refs
git --git-dir ~/.agpm/cache/sources/repo.git show-ref

# Re-clone if corrupted
agpm cache clean --source repo-name
agpm install
```

### Parallel Installation Problems

**Too many concurrent operations:**
```bash
# Check system load
top -n1 | grep "load average"

# Reduce parallelism
agpm install --max-parallel 2

# Monitor Git operations
RUST_LOG="agpm::git=debug" agpm install
```

**Git semaphore exhaustion:**
```bash
# Check CPU count (semaphore = 3 * cores)
nproc  # Linux
sysctl -n hw.ncpu  # macOS
echo $NUMBER_OF_PROCESSORS  # Windows

# Force sequential operations
agpm install --max-parallel 1
```

## SHA-Based Optimization Issues

AGPM v0.3.2+ uses centralized version resolution and SHA-based worktrees for optimal performance. Here are common issues:

### Version Resolution Failures

```bash
# Check if version constraint is valid
agpm validate --resolve

# Debug version resolution
RUST_LOG="agpm::resolver::version_resolver=debug" agpm install

# Check available tags in repository
git ls-remote --tags https://github.com/org/repo.git
```

### SHA Collision or Invalid SHA

```bash
# Clean resolved SHA cache
agpm cache clean --all

# Force fresh resolution
agpm install --no-cache

# Verify repository integrity
agpm cache list
```

### Worktree Deduplication Issues

```bash
# Check if worktrees are being reused properly
ls ~/.agpm/cache/worktrees/

# View worktree reuse in logs
RUST_LOG="agpm::cache=debug" agpm install

# Clean stale SHA-based worktrees
agpm cache clean --worktrees
```

### Constraint Resolution Problems

```bash
# Test constraint manually
agpm validate --resolve

# Check for complex constraints that might fail
# Example: version = ">=1.0.0, <2.0.0, !=1.5.0"

# Simplify constraints temporarily
# Change complex constraint to exact version for testing
```

## Performance Issues

### Slow Installation

```bash
# Check network speed
ping github.com

# Use parallel operations with worktrees
agpm install --max-parallel 8

# Use cache (worktrees reuse bare repos)
agpm install  # Second run uses cache

# Check worktree overhead
RUST_LOG="agpm::cache=debug" agpm install
```

### High Memory Usage

```bash
# Limit parallelism (reduces concurrent worktrees)
agpm install --max-parallel 2

# Clean cache and worktrees regularly
agpm cache clean
agpm cache clean --worktrees

# Monitor worktree count
find ~/.agpm/cache/worktrees/ -maxdepth 1 -type d | wc -l
```

## Debugging

### Enable Debug Logging

```bash
# Verbose output
RUST_LOG=debug agpm install

# Focus on Git operations
RUST_LOG="agpm::git=debug" agpm install

# Focus on cache operations
RUST_LOG="agpm::cache=debug" agpm install

# Trace-level logging
RUST_LOG=trace agpm install

# Log to file
RUST_LOG=debug agpm install 2> debug.log
```

### Check Git Operations

```bash
# Test git commands directly
GIT_TRACE=1 git clone https://github.com/org/repo.git

# Test bare clone (AGPM method)
GIT_TRACE=1 git clone --bare https://github.com/org/repo.git /tmp/test.git

# Test worktree creation
cd /tmp/test.git
GIT_TRACE=1 git worktree add /tmp/work main

# Check git config
git config --list

# Verify worktree support
git --version  # Should be >= 2.5
```

### Validate Configuration

```bash
# Check manifest syntax
agpm validate

# Check with resolution
agpm validate --resolve

# Check lockfile consistency
agpm validate --check-lock
```

## Getting Help

If you're still having issues:

1. Check the [FAQ](faq.md)
2. Search [existing issues](https://github.com/aig787/agpm/issues)
3. Create a [new issue](https://github.com/aig787/agpm/issues/new) with:
   - AGPM version: `agpm --version`
   - Platform: Windows/macOS/Linux
   - Error message
   - Debug output: `RUST_LOG=debug agpm [command]`
   - Relevant config files (remove sensitive data)
