# ArchOxide

ArchOxide is a keyboard-driven TUI for managing Arch Linux packages, Flatpak apps, and managed AppImages. Use `pacman` for configured repositories, `yay` for optional AUR support, and `flatpak` for configured Flatpak installations. AppImages use local files or direct HTTPS sources. Preserve source identity and scope when building install, removal, or update operations.

Use Neovim-style navigation with defaults adapted from the maintainer’s AstroNvim setup. `gg` and `Ctrl+Home` jump to the first row. `G` and `Ctrl+End` jump to the last row. Persist themes, layout, fonts, and package source switches in `$XDG_CONFIG_HOME/archoxide/config.toml` with the `~/.config` fallback. Fontconfig provides the font picker, and `--kitty` applies the saved terminal font on launch.

ArchOxide is a Rust 2024 binary crate named `archoxide`. The Cargo manifest is `Cargo.toml`, and the executable entry point is `src/main.rs`. Keep `Cargo.lock` under version control so application builds resolve the same dependency versions.

## Transactions

Keep package-manager prompts and live output in the terminal popup. `src/transaction_terminal.rs` coordinates the popup, `src/pty.rs` handles the child terminal, and `src/terminal.rs` drives input and redraws. Refresh the affected inventory after a transaction so the visible state reflects partial successes as well as completed operations. Use fixture commands or a disposable installation when verifying changes that would install, update, or remove packages.

## Development commands

- `cargo run --locked` builds and runs the executable in a terminal.
- `cargo fmt --check` verifies formatting.
- `cargo clippy --locked --all-targets -- -D warnings` runs the same lint check as CI.
- `cargo test --locked` runs the test suite.
- The [CI workflow](.github/workflows/ci.yml) also builds the binary and answers a fixture package-manager prompt through the terminal popup.

## Pull request reviews

In Claude Code, run `/archoxide-pr-review 123` or pass a PR URL. With no argument, the skill reviews the PR for the current branch. [Shared PR review skill](.agents/skills/archoxide-pr-review/SKILL.md)
