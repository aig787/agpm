# Claude Code Agents for CCPM

## Agent Architecture

### Quick Reference
- **rust-linting-expert** (haiku): Format & simple clippy fixes → delegates complex work
- **rust-test-fixer** (sonnet): Fix test failures → delegates refactoring/memory issues  
- **rust-expert** (sonnet): General Rust development, refactoring, implementation
- **rust-troubleshooter-opus** (opus-4-1): Deep debugging, memory issues, UB, complex problems

### Delegation Mechanism

Agents recognize their limits and exit with clear delegation requests:

```
rust-linting-expert (haiku)
├→ Exits requesting → rust-expert (for refactoring)
└→ Exits requesting → rust-troubleshooter-opus (for memory/UB)

rust-test-fixer (sonnet)
├→ Exits requesting → rust-expert (for implementation)
└→ Exits requesting → rust-troubleshooter-opus (for race conditions)

rust-expert (sonnet)
└→ Exits requesting → rust-troubleshooter-opus (for deep debugging)
```

**How it works**: 
1. Agent recognizes issue beyond its scope
2. Provides detailed context about the problem
3. Prints: "Please run: /agent [specialist-name]"
4. Exits cleanly
5. User runs the recommended agent

This approach is simple, explicit, and always works.

### When Each Agent Activates

**rust-linting-expert**: 
- Commands: `cargo fmt`, `cargo clippy`
- Issues: Formatting, simple warnings

**rust-test-fixer**:
- Commands: `cargo test` failures
- Issues: Assertion failures, missing test utilities

**rust-expert**:
- Tasks: New features, refactoring, API changes
- Issues: Complex implementations

**rust-troubleshooter-opus**:
- Issues: Segfaults, UB, race conditions, memory leaks
- Tools: Miri, sanitizers, deep debugging

### Design Principles
1. **Token Efficiency**: Concise prompts, surgical tool selection
2. **Clear Delegation**: Each agent knows its limits
3. **Model Hierarchy**: haiku→sonnet→opus based on complexity
4. **Automatic Handoff**: Clear templates for delegation