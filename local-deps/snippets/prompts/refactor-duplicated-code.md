# Refactor Duplicated Code - Systematic Analysis

## Objective
Perform a module-by-module analysis to identify duplicated code segments and patterns that would benefit from refactoring into shared modules or utilities.

## Analysis Process

### Step 1: Module-by-Module Code Review
Go through each module in the following order and identify:
1. Duplicated code blocks (>5 lines similar code)
2. Similar patterns with minor variations
3. Common error handling patterns
4. Repeated helper functions
5. Similar struct implementations or trait patterns

#### Modules to analyze:
- `src/cli/*.rs` - Command implementations
- `src/core/*.rs` - Core functionality
- `src/git/*.rs` - Git operations
- `src/manifest/*.rs` - Manifest handling
- `src/lockfile/*.rs` - Lockfile management
- `src/resolver/*.rs` - Dependency resolution
- `src/source/*.rs` - Source repository operations
- `src/utils/*.rs` - Utility functions
- `src/mcp/*.rs` - MCP server management
- `tests/*.rs` - Integration tests

### Step 2: Cross-Module Analysis
After analyzing individual modules, perform a cross-module sweep to identify:
1. **Shared patterns across modules**: Similar implementations in different contexts
2. **Common abstractions**: Functionality that appears in 3+ modules
3. **Interface duplications**: Similar APIs or function signatures
4. **Data flow patterns**: Repeated transformation or validation logic

#### Cross-Module Focus Areas:
- Error handling patterns used across CLI, core, and utils
- Path manipulation appearing in multiple modules
- Configuration reading/writing patterns
- Async operation patterns across different commands
- Validation logic repeated in manifest, lockfile, and resolver
- Test setup patterns shared across integration tests

### Step 3: Pattern Identification
For each module AND cross-module analysis, document:
1. **Exact duplications**: Code that is copy-pasted
2. **Near duplications**: Similar code with minor parameter differences
3. **Structural patterns**: Similar flow/logic with different types
4. **Cross-module duplications**: Same code appearing in multiple modules
5. **Semantic duplications**: Different implementations achieving same goal

### Step 4: Refactoring Opportunities
For each identified duplication, propose:
1. **Target location**: Where the shared code should live
2. **Abstraction type**: Function, macro, trait, or generic
3. **Impact assessment**: Which modules would be affected
4. **Complexity rating**: Simple/Medium/Complex refactor

## Analysis Templates

### Individual Module Template

#### Module: [module_name]

##### Duplicated Code Found:
```rust
// Location 1: file:line
// code snippet

// Location 2: file:line  
// code snippet
```

##### Refactoring Proposal:
- **Extract to**: `src/utils/[new_module].rs` or existing module
- **Suggested name**: `function_or_trait_name`
- **Pattern type**: Function/Trait/Macro/Generic
- **Benefits**: 
  - Reduces code by X lines
  - Improves maintainability
  - Enables better testing

### Cross-Module Duplication Template

#### Pattern: [pattern_name]

##### Modules Affected:
- `src/cli/install.rs` - lines X-Y
- `src/cli/update.rs` - lines A-B  
- `src/source/mod.rs` - lines M-N

##### Common Functionality:
Description of what the duplicated code achieves

##### Code Examples:
```rust
// From src/cli/install.rs:45
// code snippet

// From src/cli/update.rs:78
// code snippet  

// From src/source/mod.rs:123
// code snippet
```

##### Unified Solution:
- **Extract to**: `src/utils/[new_module].rs` or existing module  
- **Proposed API**: Function signature or trait definition
- **Migration strategy**: How to update all affected modules
- **Estimated reduction**: X lines across Y modules
- **Testing approach**: Unit tests for shared functionality
- **Priority**: High/Medium/Low based on frequency and impact

## Priority Areas to Focus On

### 1. Error Handling Patterns
Look for repeated error creation, context adding, and error conversion patterns that could be:
- Extracted into error helper functions
- Converted to macros for common error patterns
- Unified through trait implementations

### 2. File I/O Operations
Identify duplicated:
- File reading/writing patterns
- Path manipulation and validation
- Atomic file operations
- Directory creation and management

### 3. Git Command Execution
Find repeated patterns in:
- Git command building
- Output parsing
- Error handling for git operations
- Repository state checking

### 4. Test Utilities
In integration tests, look for:
- Test setup/teardown patterns
- Mock data creation
- Assertion helpers
- Temporary directory management

### 5. CLI Output Formatting
Identify repeated:
- Progress bar creation
- Status message formatting
- Color/styling applications
- Table or list formatting

### 6. Configuration Handling
Look for duplicated:
- TOML parsing patterns
- Configuration validation
- Default value handling
- Path resolution

### 7. Async Patterns
Find repeated:
- Tokio runtime setup
- Concurrent operation patterns
- Async error handling
- Future composition patterns

## Specific Anti-patterns to Find

1. **Copy-paste programming**: Exact code duplicates
2. **Reinvented wheels**: Custom implementations of standard library features
3. **Scattered constants**: Same values defined in multiple places
4. **Repeated type conversions**: Same From/Into implementations
5. **Duplicated validation**: Same checks in multiple locations
6. **Parallel hierarchies**: Similar structures maintained separately

## Output Format

### Summary Report:
```
Total modules analyzed: X
Total duplications found: Y
Estimated lines that could be removed: Z
Recommended new shared modules: N

Priority refactoring tasks:
1. [High] Extract common error handling to error_utils
2. [High] Create shared git command builder
3. [Medium] Unify file I/O operations
4. [Low] Consolidate test utilities
```

### Detailed Findings:
For each duplication found, provide:
1. Current locations (file:line references)
2. Lines of code duplicated
3. Proposed solution
4. Implementation complexity
5. Testing requirements

## Success Metrics

- **Code reduction**: Target 10-20% reduction in total lines
- **Test coverage**: Maintain or improve current coverage
- **Performance**: No regression in benchmark times
- **Clarity**: Improved code readability and maintainability
- **Reusability**: New utilities usable across modules

## Cross-Module Analysis Methodology

### Step 1: Pattern Mining
1. Use ripgrep to find common patterns across modules:
   - Function signatures with similar names
   - Error creation patterns  
   - File I/O operations
   - Path manipulations
   - Validation logic

### Step 2: Semantic Analysis  
1. Group similar functionality even if implementation differs
2. Identify conceptual duplications (same goal, different code)
3. Look for parallel evolution (features added similarly in multiple places)

### Step 3: Dependency Mapping
1. Map which modules could share utilities
2. Identify circular dependency risks
3. Plan extraction order to avoid breaking changes

### Step 4: Impact Assessment
1. Count occurrences across codebase
2. Estimate maintenance burden of duplication
3. Calculate potential code reduction
4. Assess testing complexity reduction

## Implementation Strategy

1. **Phase 1**: Cross-module duplications (highest impact)
   - Extract shared patterns used in 3+ modules
   - Focus on error handling and file I/O first
   
2. **Phase 2**: Module-specific duplications  
   - Clean up within-module duplications
   - Extract to module-local helpers
   
3. **Phase 3**: Architectural patterns
   - Unify similar command structures
   - Consolidate validation approaches
   
4. **Phase 4**: Test utilities
   - Extract common test helpers
   - Create test fixture generators
   
5. **Phase 5**: Final optimization
   - Review extracted utilities for further consolidation
   - Document shared patterns thoroughly

## Notes

- Prioritize refactorings that affect the most modules
- Consider backward compatibility for public APIs
- Ensure refactored code maintains or improves testability
- Document new shared utilities thoroughly
- Consider performance implications of abstractions
- Follow Rust idioms and best practices