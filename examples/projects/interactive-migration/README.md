# Interactive Migration Test Project

This project simulates an old AGPM installation to test the interactive migration feature.

## What's included

- **Agent at old path**: `.claude/agents/test-agent.md` (should be `.claude/agents/agpm/test-agent.md`)
- **Lockfile with old paths**: `agpm.lock` points to the old location
- **Old gitignore format**: `.gitignore` has `# AGPM managed entries` section

## Testing the interactive migration

1. Build AGPM from the repo root:
   ```bash
   cargo build
   ```

2. Navigate to this directory:
   ```bash
   cd examples/projects/interactive-migration
   ```

3. Run install:
   ```bash
   ../../../target/debug/agpm install
   ```

4. You should see a prompt like:
   ```
   Legacy AGPM format detected!

   → Found 1 resources at old paths:
       • .claude/agents/test-agent.md

   → Found legacy managed section in .gitignore

   The new format uses agpm/ subdirectories for easier gitignore management.

   Would you like to migrate to the new format now? [Y/n]:
   ```

5. Press Enter (or type "y") to migrate, or "n" to skip.

## Expected results after migration

- Agent moved to: `.claude/agents/agpm/test-agent.md`
- `.gitignore` updated with simple directory patterns (no managed section markers):
  ```
  .claude/agents/agpm/
  .claude/commands/agpm/
  .agpm/
  agpm.private.toml
  agpm.private.lock
  ```
- `agpm.lock` updated with new `installed_at` path

## Resetting the test

To reset and test again:
```bash
git checkout -- .
```
