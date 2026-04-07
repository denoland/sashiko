# False Positive Guide

## Common False Positives to Filter Out

### Rust-Specific
- `unwrap()` in test code is fine
- `unsafe` blocks with correct SAFETY comments and sound invariants
- `clone()` on small types (String, PathBuf) — not a real performance issue
- Missing error handling in example/test code

### Deno-Specific
- Permission checks in internal-only code paths (not reachable from user JS)
- Platform-specific code behind proper `cfg()` guards
- Intentional divergence from Node.js behavior (documented in comments)
- `todo!()` or `unimplemented!()` in draft/WIP PRs (flag but Low severity)

### General
- Style preferences (tabs vs spaces, brace placement)
- Import ordering
- Variable naming that follows existing codebase conventions
- Comments explaining "why" — don't flag these as unnecessary

## When in Doubt
- If you're less than 70% confident an issue is real, don't report it
- If the issue requires deep domain knowledge you don't have, mark it Low
- If similar patterns exist elsewhere in the codebase without issues, skip it
