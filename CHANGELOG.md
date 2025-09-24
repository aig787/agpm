## [](https://github.com/aig787/ccpm/compare/v0.3.1...v) (2025-09-24)

### ⚠ BREAKING CHANGES

* Cache API completely rewritten with new worktree model

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* refactor(installer)!: rewrite with parallel processing and worktree support

- Extract installation logic from CLI into dedicated installer module
- Add parallel resource installation with configurable concurrency
- Support --max-parallel flag (default: max(10, 2 × CPU cores))
- Improve error handling and progress reporting
- Add comprehensive test coverage for parallel operations
* Installation API completely redesigned

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* refactor(progress): simplify progress reporting system

- Remove complex multi-layer progress tracking
- Streamline spinner and progress bar management
- Improve TTY vs non-TTY mode handling
- Add better test coverage for progress updates

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* feat(git): enhance command operations with better error handling and worktree support

- Improve CommandBuilder with better output capture
- Add comprehensive worktree operation support
- Enhanced error handling with detailed context
- Better credential and authentication management
- Add tests for new git operations

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* refactor(resolver): improve dependency resolution for new architecture

- Enhanced local vs Git source detection
- Better pattern resolution with manifest context
- Improved error messages and validation
- Support for new cache architecture
- Optimize source repository management

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* feat(cli): enhance commands with better validation and configuration

- Improve add command with better source/dependency handling
- Update cache command for new worktree architecture
- Enhanced validation with --resolve flag support
- Add global config helpers for better path management
- Minor improvements to list command output

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* feat(commands): add execute and squash commands, update existing commands

- Add new /execute command for running saved commands
- Add new /squash command for intelligent commit squashing
- Add fix-failing-tests helper documentation
- Update pr-self-review with better analysis
- Update documentation commands with new agent support
- Minor updates to all commands for consistency

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* feat(agents): update Rust agent tool configurations

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* test: add comprehensive integration tests for parallel operations

- Add stress tests for parallel installations
- Improve test fixtures and utilities
- Update existing tests for new architecture
- Remove obsolete worktree simple test
- Add better test coverage for cross-platform scenarios

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* docs: update architecture documentation for v0.3.x refactor

- Update CLAUDE.md with new worktree architecture
- Expand architecture.md with detailed design decisions
- Update command reference with new features
- Improve configuration documentation
- Update installation guide
- Enhance contributing guidelines
- Update example setup script

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* fix(core): improve error handling and resource management

- Enhanced error types with better context
- Improve resource iterator for pattern handling
- Add lockfile checksum validation
- Minor manifest parsing improvements
- Fix hooks merge behavior
- Update main entry point for new architecture

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* fix(commands): update squash to analyze code changes for attribution

- Attribution based on actual code diff analysis, not squash operation
- Apply commit.md rules: >50% AI = co-author, 25-50% = note, <25% = none
- Clarify that squashing itself doesn't require attribution

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* feat(commands): add commit range support to pr-self-review command

Enable pr-self-review to accept commit ranges (e.g., main..HEAD, abc123..def456)
in addition to single commits. Updates parsing logic to handle both formats
and provides appropriate git commands for each review target type.

Co-authored-by: Claude <noreply@anthropic.com>

* fix: Manual release triggered with patch version bump

* docs: enhance documentation for hooks, installer, and core modules

Add comprehensive docstrings and examples for public APIs including:
- Hook configuration and validation functions with usage examples
- Lockfile and manifest resource collection methods
- Resolver source_manager field documentation
- Remove obsolete ProgressConfig enum documentation
- Update installer tests to use new install_resources API

Co-authored-by: Claude <noreply@anthropic.com>

* refactor: reorganize Claude commands and add checkpoint functionality

- Add new checkpoint command for safe Git state preservation
- Move prompts to dedicated .claude/snippets/prompts/ directory
- Extract attribution logic to reusable snippet
- Update execute command to support new directory structure
- Integrate checkpoint safety into squash command
- Add MCP server entries to .gitignore

Co-authored-by: Claude <noreply@anthropic.com>

* refactor: improve test stability and documentation examples

- Rename integration test file to avoid Windows UAC elevation issues
- Increase lock timeout tolerance for slower systems in cache tests
- Update all Git and source documentation examples to use platform-appropriate temp directories
- Improve test assertion flexibility for cross-platform compatibility
- Document Windows UAC elevation gotcha for test naming

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* fix: enable pattern tests and improve local pattern resolution

- Fixed 2 ignored pattern tests in resolver module by improving path handling
- Enhanced local pattern dependency resolution to support absolute paths
- Tests now use absolute paths instead of changing working directory
- Added clippy allow directives for Windows-specific set_readonly(false) calls
- Applied cargo fmt and clippy fixes across the codebase

All 37 resolver tests now pass including previously ignored pattern tests.

Co-authored-by: Claude <noreply@anthropic.com>

* feat(resolver): add centralized version resolver for SHA-based caching

Introduces VersionResolver for efficient two-phase resolution strategy:
- Collection phase gathers all unique (source, version) pairs
- Resolution phase batch resolves versions to SHAs
- Enables SHA-based worktree deduplication
- Supports semver constraint resolution
- Minimizes Git operations through batching

Refactors cache module to use SHA-based worktrees:
- One worktree per unique commit (not per version)
- Automatic deduplication when multiple refs point to same commit
- WorktreeState enum tracks creation status (Pending/Ready)
- Per-worktree locking for parallel operations

🤖 Generated with Claude Code (https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* refactor(core): improve architecture and error handling across modules

Major refactoring across core modules:
- Enhanced error handling with better context propagation
- Improved CLI module structure and command organization
- Streamlined config management with validation
- Optimized Git operations with better test coverage
- Refactored installer to use new VersionResolver
- Simplified source and manifest handling
- Enhanced utilities for cross-platform support

Key improvements:
- Reduced code duplication across modules
- Better separation of concerns
- More consistent error messages
- Improved test coverage for edge cases

🤖 Generated with Claude Code (https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* refactor(modules): update supporting modules for consistency

Updates supporting modules to align with core refactoring:
- Hooks module: improved merge logic and error handling
- Markdown module: better parsing and validation
- MCP module: enhanced server configuration handling
- Pattern module: optimized glob pattern resolution
- Version module: improved constraint parsing and comparison
- Test utilities: streamlined test infrastructure

These changes ensure consistency across the entire codebase
and leverage the new architecture improvements.

🤖 Generated with Claude Code (https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* test: refactor integration tests for improved stability and coverage

Major test suite improvements:
- Refactored test infrastructure for better reliability
- Enhanced test helpers and fixtures
- Improved parallel test execution safety
- Added comprehensive coverage for new resolver architecture
- Removed deprecated worktree_simple test
- Updated tests to use new SHA-based caching approach

Test improvements include:
- Better isolation between test cases
- More robust error scenario testing
- Enhanced cross-platform test coverage
- Improved stress test scenarios
- Cleaner test data management

🤖 Generated with Claude Code (https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* docs: update documentation and build configuration for v0.3.2

Documentation and configuration updates:
- Updated CLAUDE.md with v0.3.2 architecture details
- Enhanced README with latest features
- Improved architecture documentation for SHA-based caching
- Updated command reference and user guide
- Added troubleshooting for common issues
- Enhanced versioning documentation
- Updated Claude commands for multi-commit support
- Upgraded dependencies including dashmap to v6.1
- Improved Makefile with better tool installation

Key documentation additions:
- Centralized VersionResolver architecture
- SHA-based worktree deduplication strategy
- Two-phase resolution process
- Improved concurrency model
- Enhanced error handling patterns

🤖 Generated with Claude Code (https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* style: fix multi-line if-let chain formatting across codebase

* docs: add beta disclaimer and improve installation instructions

- Add prominent beta software warning at top of README
- Reorganize installation instructions with Cargo as recommended option
- Add platform-specific pre-built binary installation sections
- Include ARM64 Linux binary installation option
- Update all platforms to use consistent ~/.ccpm/bin directory
- Improve Windows installation to use user-level directory and PATH

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* test: fix Windows path handling in update network failure test

- Convert backslashes to forward slashes for file:// URLs on Windows
- Trim manifest content to avoid extra newlines
- Ensures cross-platform compatibility for path-based tests

🤖 Generated with Claude Code (https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>

* Large refactor to dependency resolution and caching (#5) ([41be284](https://github.com/aig787/ccpm/commit/41be284b74031eda9bc6a8a0e45684ed61d67511)), closes [#5](https://github.com/aig787/ccpm/issues/5)

### Features

* **markdown:** add resilient frontmatter parsing with graceful fallback ([c913d09](https://github.com/aig787/ccpm/commit/c913d09b393a7fc9a959a923dfe34c5cda6916d1))

### Bug Fixes

* **ci:** enable Git authentication in release workflow ([9c0fec6](https://github.com/aig787/ccpm/commit/9c0fec6d8628ff09c4fd784b5f5f23a7c216de6c))
* Manual release triggered with patch version bump ([f17b83a](https://github.com/aig787/ccpm/commit/f17b83a6f719484bce2e8db844df5a1776fce068))
* Manual release triggered with patch version bump ([df72968](https://github.com/aig787/ccpm/commit/df729682a8c1298c15ae6c78444b777afc148fcf))
* **release:** remove semantic-release from git configuration step ([4dd4b83](https://github.com/aig787/ccpm/commit/4dd4b83c3c4773ac7cbc23c378af0cb753584624))
* **release:** replace semantic-release with manual version control ([dec063a](https://github.com/aig787/ccpm/commit/dec063a7ff12328f51537370bf682c60e86d6e19))
* **resolver:** update doctests to match new method signatures ([c27d366](https://github.com/aig787/ccpm/commit/c27d3660e954ec6da2badf8ab43f3f16e442f0f2))
