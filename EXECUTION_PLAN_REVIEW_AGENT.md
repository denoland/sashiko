# Execution Plan: Sashiko Review Agent

## Phase 1: Environment & Client
1.  **Binaries**: Setup `src/bin/review.rs`.
2.  **Gemini Client**: Implement `src/agent/client.rs` with support for `FunctionCalling` and `Content` arrays.
3.  **Config**: Ensure `SASHIKO_PROMPTS_DIR` and `GIT_REPO_PATH` are configurable.

## Phase 2: Tooling Implementation
1.  **Git Ops**: Extend `src/git_ops.rs` or add to `src/agent/tools.rs`:
    -   `git_grep`, `git_show` (path specific), `git_blame`.
2.  **Filesystem**: `read_file_lines`, `list_dir`.
3.  **Metadata**: Tool to fetch Patchset details (Subject, Author, Files touched).

## Phase 3: Prompt Registry & Selection
1.  **Scanner**: Implement logic to detect which `review-prompts/*.md` files apply to a given patch.
    -   Match `net/` -> `networking.md`.
    -   Grep `spin_lock` -> `locking.md`.
2.  **Assembler**: Method to build the initial "Mega-Prompt" or at least the System Instruction + Context Block.

## Phase 4: Agent Core Loop
1.  **ReAct Loop**: Implement the loop that handles `ModelResponse::FunctionCall`.
2.  **Task Tracking**: Implement a basic in-memory `TodoWrite` tracker to help the model not get lost in `CS-001` tasks.
3.  **Error Handling**: Handle tool failures (e.g. file not found) by reporting back to the LLM.

## Phase 5: Storage & Reporting
1.  **Database**: Save results to `reviews` table.
2.  **Interaction Log**: Save full conversation to `ai_interactions` for debugging/replays.

## Phase 6: Testing
1.  **Integration Test**: Use a known LKML patch (e.g. a simple BPF or Networking fix).
2.  **Verification**: Ensure the agent actually calls `git_blame` and `read_file_lines` as instructed by `CS-001`.
