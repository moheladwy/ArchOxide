# Contributing to ArchOxide

## Report an issue

Search existing issues and pull requests, then choose the matching [issue form](.github/ISSUE_TEMPLATE). For a bug, include the ArchOxide version or commit, distribution, terminal, and package source where relevant. List the view and keys you used, what you expected, and what happened. Remove secrets and personal data from logs and screenshots. Report suspected vulnerabilities through the [security policy](SECURITY.md).

## Develop and verify

Create a branch from `main`. Use a current stable Rust toolchain and run `cargo run --locked` from an interactive terminal. See the [README](README.md#install-and-run) for optional Arch Linux tools.

Run the checks relevant to your change before opening a PR.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
cargo test --locked
```

Unit tests under `tests/unit/` mirror their source modules. Use fixture commands or a disposable installation for package operations. For TUI changes, verify the affected keys, views, prompts, and terminal behavior. For AUR metadata changes, regenerate `.SRCINFO` with `makepkg --printsrcinfo > .SRCINFO`.

## Open a pull request

Target `main` and use the [PR template](.github/pull_request_template.md). Explain what changed and why, link related issues, and list the checks you ran and their results. Call out user-facing changes to navigation, package operations, or saved settings. Keep each PR focused and update the relevant documentation when behavior changes. The [CI workflow](.github/workflows/ci.yml) runs formatting, linting, build, test, and terminal smoke checks on PRs.
