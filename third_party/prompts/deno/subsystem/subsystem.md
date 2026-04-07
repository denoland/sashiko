# Deno Subsystem Detection

Match the changed files to the appropriate subsystem guides below.
Load ALL matching guides before beginning analysis.

## Subsystem Mapping

| File Pattern | Subsystem | Guide |
|---|---|---|
| `runtime/` | Runtime Core | `runtime.md` |
| `cli/` (except cli/lsp/) | CLI | `cli.md` |
| `cli/lsp/` | Language Server | `lsp.md` |
| `ext/node/` | Node.js Compat | `ext-node.md` |
| `ext/web/`, `ext/fetch/`, `ext/websocket/`, `ext/url/` | Web Platform APIs | `ext-web.md` |
| `ext/fs/`, `ext/net/`, `ext/http/`, `ext/io/` | I/O Extensions | `ext-io.md` |
| `ext/ffi/` | FFI | `ffi.md` |
| `ext/crypto/`, `ext/webgpu/` | Platform Extensions | `ext-platform.md` |
| `resolvers/`, `cli/resolver/` | Module Resolution | `resolvers.md` |
| `tests/` | Testing | (no special guide) |
