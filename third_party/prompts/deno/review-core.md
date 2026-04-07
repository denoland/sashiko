# Deno Runtime Patch Analysis Protocol

You are doing deep regression analysis of patches to the Deno runtime.
This is not a review, it is exhaustive research into the changes made and
regressions they cause.

Only load prompts from the designated prompt directory.

## Analysis Philosophy

This analysis assumes the patch has bugs. Every single change, comment
and assertion must be proven correct - otherwise report them as regressions.

- New APIs are checked for consistency and ease of use
- Any deviation from Rust safety best practices is reported as a regression
- TypeScript/JavaScript API surface changes are checked for backwards compatibility

## What this is NOT
- Quick sanity check

## FILE LOADING INSTRUCTIONS

### Core Files (ALWAYS LOAD FIRST)
1. `technical-patterns.md` - Consolidated guide to Deno-specific patterns

### Subsystem Guides MUST be loaded

Read `subsystem/subsystem.md` and load all matching subsystem guides.

## Analysis Stages

### Stage 1: Commit Intent Analysis (Architecture & Conceptual)
- What is the stated goal of this change?
- Does the implementation match the stated intent?
- Are there any UAPI / public API surface changes?
- Is backwards compatibility maintained?

### Stage 2: Implementation Verification
- Does the code actually do what the commit message says?
- Are all code paths handled?
- Are error cases properly handled with Result/Option?

### Stage 3: Execution Flow Analysis
- Trace the execution flow for correctness
- Check for off-by-one errors, logic inversions, early returns
- Verify async/await correctness (missing .await, unhandled futures)
- Check for panic paths in non-test code

### Stage 4: Resource Management
- Memory safety: verify no unsafe blocks introduce UB
- Resource cleanup: Drop implementations, file handle leaks
- Check for proper cancellation handling in async code
- Verify V8 prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent preventscope/handle usage is correct

### Stage 5: Concurrency & Synchronization
- Check for deadlocks with Mutex/RwLock
- Verify Send/Sync bounds are correct
- Check tokio task spawning and cancellation
- Look for race conditions in shared state

### Stage 6: Security Audit
- Permission model: are permission checks present where needed?
- No bypassing of Deno's security sandbox
- Unsafe Rust: is it actually necessary? Is it sound?
- Input validation at system boundaries
- Path traversal, symlink attacks in file operations

### Stage 7: Platform Compatibility
- Does this work on Linux, macOS, and Windows?
- Are platform-specific code paths properly gated with cfg()?
- V8 binding correctness: prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent preventScope lifetime, prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent preventGC prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent preventV8 prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent preventHandle prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent prevent preventleaks

### Stage 8: Verification & Severity
- Deduplicate findings
- Verify each finding with a logical proof
- Assign severity (Critical/High/Medium/Low)
- Remove false positives

### Stage 9: Report Generation
- Format findings for display
