# Execution requirements

Before modifying any code, you MUST:

1. Read all directly related files
2. Trace call graph at least 2 levels up and down
3. Identify shared types, traits, and interfaces used by the target code
4. Search for all references of modified symbols
5. Check tests touching the same logic
6. Verify no duplicated logic exists elsewhere in the repo

Use search patterns:
- function name
- struct name
- trait name
- module path
- error types
- serde structs

# Completion requirements

Do NOT stop after partial fixes.

If a vulnerability or bug spans multiple modules:
- continue until full fix is applied everywhere
- ensure consistency across modules

Do NOT output partial patches without explanation.

If task scope expands:
- continue working
- summarize expanded scope

# Autonomy rules

Do NOT ask for permission for:

- reading files
- searching repository
- tracing references
- identifying related modules
- updating tests
- fixing the same bug pattern in multiple files
- applying consistent refactors required for correctness
- adding missing validation where vulnerability already confirmed

Assume permission is granted for all safe code analysis actions.

# Required workflow

1. locate entry point
2. identify trust boundary
3. trace data flow
4. identify mutation points
5. apply fix
6. propagate fix to all equivalent paths
7. re-check original attack path

# Constraints

Do NOT:
- invent APIs not present in the repo
- rewrite unrelated code
- change formatting without reason
- introduce new dependencies unless required for security
- modify public interfaces without necessity

# If unable to complete task

Explain:

- which file is missing
- which symbol definition is unresolved
- which dependency blocks completion
- what additional context is required

Never stop silently.