---
name: commit
license: MIT
description: >
  Create clean Git history using Conventional Commits grouped by coherent
  development intent rather than strict atomicity. Use when committing changes,
  preparing commits, or deciding how a working-tree diff should be split.
---

# Coherent Commits

Create commits around **coherent development intent**, not strict atomicity.

Every commit MUST follow Conventional Commits:

```text
<type>(<scope>): <description>
```

## Group by intent

A commit SHOULD represent one understandable change.

Related implementation, tests, refactors, configuration, documentation, and other
supporting changes MAY stay together when they serve the same intent.

Unrelated intents MUST be split into separate commits.

Do not split changes merely to make commits atomic.

## Commit at natural boundaries

Commit when the current changes form a meaningful checkpoint, such as when a
feature, fix, or refactor is complete, before switching concerns, or before a
risky change.

Prompt boundaries, file count, diff size, and elapsed time are NOT commit
boundaries.

## Describe the result

Commit messages MUST describe the resulting software change, not the development
process.

Prefer:

```text
feat(auth): implement login flow
fix(editor): preserve selection during autosave
refactor(api): simplify response handling
```

Avoid vague process-oriented messages such as:

```text
fix: fix stuff
chore: update files
chore: ai changes
```

## Decide whether to split

If the whole diff can be accurately described by one Conventional Commit message,
keep it together.

Otherwise, split it by coherent intent.
