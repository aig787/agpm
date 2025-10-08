# Git Commit Attribution Analysis Logic

## Automatic AI Contribution Analysis

When determining AI contribution percentage, analyze the diff to estimate AI-generated content using these indicators:

#### Strong AI Indicators (high weight)
- New files with 100+ lines of boilerplate/template code
- Comprehensive documentation blocks with consistent formatting
- Systematic error handling across multiple functions
- Complete test suites with edge cases
- Multi-language configurations (CI/CD workflows, Docker, etc.)
- Repetitive patterns with consistent naming conventions

#### Mixed Indicators (medium weight)
- Refactoring with consistent style changes
- Adding type definitions or interfaces
- Implementing standard patterns (singleton, factory, etc.)
- Configuration updates with detailed comments

#### Human Indicators (negative weight)
- Single-line fixes or small tweaks (<5 lines)
- Business-specific logic or domain knowledge
- Hotfixes addressing specific bugs
- Custom regex patterns or complex conditionals
- TODO comments or debugging code
- Inconsistent formatting or style
- Trial-and-error patterns (multiple similar attempts)

#### Automated Tool Indicators (no attribution)
- Changes from `cargo fmt` or `rustfmt`
- Changes from `cargo clippy --fix`
- Dependency updates from `cargo update` or similar
- Any changes that are purely whitespace/formatting
- Auto-generated files or tool outputs

### Contextual Analysis
- Check file history: new files vs modifications
- Line count ratio: added vs modified vs deleted
- Complexity: simple changes vs architectural additions
- Consistency: uniform style suggests AI generation
- Completeness: AI tends to handle edge cases comprehensively

### Attribution Thresholds

Based on the calculated AI contribution percentage:
- **>50% AI-generated**: Suggests co-author attribution
  ```
  Co-authored-by: Claude <noreply@anthropic.com>
  ```
- **25-50% AI-generated**: Suggests contribution note
  ```
  🤖 Generated with Claude assistance
  ```
- **<25% AI-generated**: No attribution needed
- **Automated tool changes**: No attribution (regardless of who ran the tool)

### Implementation Notes

When implementing this logic:
1. Analyze the git diff to categorize changes
2. Weight each indicator based on its category
3. Calculate the overall AI contribution percentage
4. Apply the appropriate attribution threshold
5. Briefly explain the attribution decision (e.g., "~70% AI-generated content, adding co-author")