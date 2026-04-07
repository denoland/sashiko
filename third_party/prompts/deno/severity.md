# Severity Levels

When identifying issues, you must assign a severity level to each finding.
Treat this task seriously. Don't unnecessarily raise the priority —
critical issues must be critical, high issues must be very damaging.
Use Medium as default and lower/raise depending on the impact.

## Critical
- **Definition**: Issues that cause data loss, security vulnerabilities, or sandbox escapes.
- **Question to ask**: Can this be exploited to escape Deno's security sandbox or corrupt data? If yes, it's critical.
- **Examples**:
    - Security sandbox bypass (permission check missing or circumventable)
    - Memory safety violation (UB in unsafe Rust)
    - Use-after-free via V8 handle misuse
    - Path traversal allowing access outside allowed directories
    - Panic in production code path reachable from user input

## High
- **Definition**: Serious issues that cause crashes, data corruption, or major functionality breakage.
- **Question to ask**: Will this cause a crash or totally wrong behavior with non-trivial probability?
- **Examples**:
    - Panic/unwrap on reachable code path
    - Resource leaks (file handles, memory, V8 handles)
    - Deadlock or livelock
    - Breaking change to stable public API without deprecation
    - Incorrect async cancellation causing lost data

## Medium
- **Definition**: Recoverable issues or non-critical regressions.
- **Examples**:
    - Performance regression on cold paths
    - Incorrect error message or error type
    - Missing error handling (error swallowed silently)
    - Platform-specific bug (works on Linux, fails on Windows)
    - Node.js compatibility deviation

## Low
- **Definition**: Style, naming, and documentation issues.
- **Examples**:
    - Unused imports or variables
    - Inconsistent naming conventions
    - Missing or incorrect documentation
    - Unnecessary clone() or allocation
    - Test coverage gaps
