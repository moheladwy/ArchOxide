# ArchOxide

ArchOxide is a keyboard-driven TUI for managing packages on Arch Linux. The MVP lists and filters installed packages, searches configured repositories and the AUR, installs and removes selected packages, and runs full system upgrades. Use `pacman` for repository operations and `yay` as the first AUR helper. Keep `pacman`-only operation available and preserve interactive package-manager prompts.

Use Neovim-style navigation with defaults adapted from the maintainer’s AstroNvim setup. Persist theme, UI density, details pane width, font family and size, and AUR enablement. Themes and layout apply in the TUI. A dedicated Kitty launch applies the optional font settings.

ArchOxide is a Rust 2024 binary crate named `archoxide`. The Cargo manifest is `Cargo.toml`, and the executable entry point is `src/main.rs`. Keep `Cargo.lock` under version control so application builds resolve the same dependency versions.

## Development commands

- `cargo run` builds and runs the executable.
- `cargo fmt --check` verifies formatting.
- `cargo clippy --all-targets` checks the application and its tests for lint warnings.
- `cargo test` runs the test suite.
