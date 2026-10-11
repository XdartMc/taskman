# Changelog

What changed in each release; a release's notes on GitHub are its section here. Versions follow Cargo's rule for 0.x: the patch number (0.1.1) for anything compatible, fixes and new features alike, and the minor (0.2.0) for breaking changes, such as changed keys, config or saved-state format, or a removed command.

## Unreleased

- First public release. taskman is a terminal UI for working on several tickets at once: each ticket gets the real `claude` or `codex` CLI in its own git worktree, with its Jira, Linear, GitHub Issues or Local files issue and its GitHub PR beside it.
- The sidebar shows which session needs you, and project agents coordinate the tickets by messaging their sessions. Sessions read their ticket and PR with `taskman status`; `taskman add` and `taskman close` work from any shell.
- Prebuilt binaries for macOS (Apple Silicon and Intel) and Linux x86_64, an install script that checks the release's `SHA256SUMS` and, with the GitHub CLI, its build attestation, and updates in the background with the same checks.
