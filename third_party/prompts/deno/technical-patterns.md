# Deno Technical Patterns Guide

## Rust Patterns

### Error Handling
- Use `anyhow::Result` for application errors, `thiserror` for library errors
- Never `unwrap()` or `expect()` in non-test code without a comment explaining why it's safe
- Propagate errors with `?` operator
- Custom error types should implement `std::error::Error`

### Unsafe Rust
- Every `unsafe` block must have a `// SAFETY:` comment explaining the invariant
- Prefer safe abstractions over raw unsafe
- V8 FFI calls are inherently unsafe — ensure HandleScope lifetimes are correct
- Check that raw pointers from V8 are valid before dereferencing

### Async Patterns
- All async functions should be cancellation-safe or documented as not
- Use `tokio::select!` carefully — dropped branches must not leak resources
- Spawned tasks should handle their own errors (don't let them panic silently)
- Use `JoinHandle` and await spawned tasks to catch panics

### Resource Management
- Implement `Drop` for types that hold OS resources
- Use RAII patterns for file handles, sockets, V8 isolates
- `Rc`/`Arc` cycles cause leaks — use `Weak` where appropriate

## Deno-Specific Patterns

### Op System (deno_core)
- Ops are the bridge between JS and Rust
- `#[op2]` attribute for defining ops
- Op parameters must be deserializable from V8 values
- Fast ops bypass serde for performance — ensure types match exactly

### Permission System
- Every file/net/env access MUST check permissions first
- Use `state.borrow::<Permissions>().check_*()` methods
- Never bypass permission checks, even in "internal" code
- Permission checks must happen before the operation, not after

### V8 Integration
- `v8::HandleScope` must outlive all handles created within it
- `v8::Global` handles prevent GC — drop them when done
- Never store `v8::Local` handles beyond their scope
- `v8::String::new()` can fail for large strings — handle the None case

### Extension System
- Extensions register ops and state with the runtime
- State stored in `OpState` via `Rc<RefCell<T>>`
- Borrow rules apply at runtime — nested borrows will panic

### Node.js Compatibility (ext/node)
- Must match Node.js behavior exactly, including edge cases
- Check Node.js docs and source for expected behavior
- Polyfilled modules should pass Node.js's own test suite

### Testing
- Integration tests in `tests/` directory
- Unit tests with `#[cfg(test)]` in source files
- Use `test_util` helpers for spawning test servers
- Tests must not depend on external network resources

## Common Pitfalls

### Memory Leaks
- V8 Global handles not dropped
- Event listeners registered but never removed
- Circular Rc/Arc references

### Race Conditions
- Shared state between ops without proper synchronization
- File system operations assumed to be atomic
- TOCTOU (time-of-check-time-of-use) in permission checks

### Platform Issues
- Windows path separators (\ vs /)
- Unix-specific APIs (chmod, symlink behavior differences)
- Line ending differences (\n vs \r\n)

### API Surface
- Public TypeScript APIs must be stable
- Deprecation before removal
- JSDoc comments for public APIs
- Web platform APIs must match the spec
