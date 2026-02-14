# Sashiko

![Sashiko Logo](static/logo.png)

> **Sashiko** (刺し子, literally "little stabs") is a form of decorative reinforcement stitching from Japan. Originally used to reinforce points of wear or to repair worn places or tears with patches, here it represents our mission to reinforce the Linux kernel through automated, intelligent patch review.

Sashiko is an automated system designed to assist in the review of Linux kernel patches. It ingests patches from mailing lists, analyzes them using AI-powered prompts, and provides feedback to help maintainers and developers ensure code quality and adherence to kernel standards.

## Features

- **Automated Ingestion**: Monitors mailing lists (using `lore.kernel.org`) for new patch submissions.
- **Manual Ingestion**: Can ingest patches from a local git repository.
- **AI-Powered Review**: Utilizes LLM models to analyze patches against subsystem-specific guidelines.
- **Self-contained**: Doesn't depend on 3rd-party tools and can work with various LLM providers.

## Prompts

Sashiko is based on the set of carefully crafted prompts to guide the AI in its reviews. These prompts were initially created by Chris Mason and are developed by the community of developers in a separate repository:

*   [**review-prompts**](https://github.com/masoncl/review-prompts)

## Prerequisites

- **Rust**: Version 1.86 or later.
- **Git**: For managing the repository and kernel tree.
- **LLM Provider API Key**: Access to an LLM provider (e.g., Google's Gemini or Anthropic's Claude).

## Setup

1.  **Clone the repository**:
    ```bash
    git clone --recursive https://github.com/rgushchin/sashiko.git
    cd sashiko
    ```
    *Note: The `--recursive` flag is important to initialize the `linux` kernel source and `review-prompts` submodules.*

2.  **Configuration**:
    Copy `Settings.toml` to customize your configuration. The default `Settings.toml` includes sections for:
    *   **Database**: SQLite database path (`sashiko.db`).
    *   **NNTP**: Server details and groups to monitor.
    *   **AI**: Provider and model selection.
    *   **Server**: API server host and port.
    *   **Git**: Path to the reference kernel repository.
    *   **Review**: Concurrency and worktree settings.

    ### Configuring the LLM Provider

    Sashiko supports multiple LLM providers (e.g. `gemini`). You must configure the provider and model in `Settings.toml`. There are no default values, so please set them explicitly.

    Example `Settings.toml` configuration for Gemini:

    ```toml
    [ai]
    provider = "gemini"
    model = "gemini-3-pro-preview"
    # Optional settings
    # max_input_tokens = 950000
    # temperature = 1.0
    ```

    You can also configure settings via environment variables using the `SASHIKO` prefix and double underscores for nesting (e.g., `SASHIKO_AI__PROVIDER=gemini`).

    **Important**: You must set the `LLM_API_KEY` environment variable with your provider's API key.
    ```bash
    export LLM_API_KEY="your_api_key_here"
    ```

    ### Claude Setup

    Sashiko supports Anthropic's Claude models via the Claude API.

    **Get an API key**: https://console.anthropic.com/

    **Configure environment**:
    ```bash
    export ANTHROPIC_API_KEY="sk-ant-..."
    # Or use the generic key (LLM_API_KEY serves as fallback):
    export LLM_API_KEY="sk-ant-..."
    ```

    **Update Settings.toml**:
    ```toml
    [ai]
    provider = "claude"
    model = "claude-sonnet-4-5"
    max_input_tokens = 180000

    [ai.claude]
    prompt_caching = true
    ```

    **Supported models**:
    - `claude-sonnet-4-5` - Recommended (200K context, balanced quality/cost/speed)
    - `claude-opus-4-5` - Highest quality (1M context)
    - `claude-haiku-4-5` - Fastest and cheapest

    **Features**:
    - Automatic prompt caching (5-minute TTL) reduces costs for repeated context
    - Full tool/function calling support for git operations
    - Automatic retry logic for rate limits and API overload
    - 200K context window for claude-sonnet-4-5 (use max_input_tokens = 180000 for safety margin)

3.  **Build**:
    ```bash
    cargo build --release
    ```

## Usage

To start the application:

```bash
cargo run --release
```

This will start the Sashiko daemon, which will begin ingesting and reviewing patches based on your configuration.

## Contributing

This project uses the Developer Certificate of Origin (DCO). All contributions must include a `Signed-off-by` line to certify that you wrote the code or have the right to contribute it.

You can automatically add this line by using the `-s` flag when committing:

```bash
git commit -s
```

## License

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
