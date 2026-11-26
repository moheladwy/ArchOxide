# ArchOxide

ArchOxide is a keyboard-driven terminal UI for managing Arch Linux packages from configured `pacman` repositories and the Arch User Repository (AUR). The Rust 2024 crate is currently a scaffold.

## MVP

- **Installed packages.** Browse installed packages and filter the list by name, version, or repository availability.
- **Search and package changes.** Search configured repositories and the AUR, install a selected package, remove an installed package, and run a full system upgrade. Use `pacman` for repository operations and `yay` for AUR support. Core repository features remain available when `yay` is absent. Package-manager prompts remain interactive, including AUR build review when offered by `yay`.
- **Neovim-style navigation.** Use `j` and `k` to move, `gg` and `G` to jump, `/` to filter or search the current view, and Space as the leader key. The planned defaults adapt the maintainer’s AstroNvim shortcuts for switching views, opening the installed list, searching, and picking themes.
- **Settings.** Edit and persist the theme, UI density, details pane width, font family and size, and AUR enablement through the Settings screen or a TOML file. The planned `archoxide --kitty` launch mode applies font settings in a dedicated Kitty window. A normal launch inherits the current terminal’s font. [Ratatui font-size FAQ](https://ratatui.rs/faq/#can-you-change-font-size-in-a-terminal-using-ratatui), [Kitty command-line options](https://sw.kovidgoyal.net/kitty/invocation/)

## Development commands

- `cargo run` builds and runs the current scaffold.
- `cargo fmt --check` verifies formatting.
- `cargo clippy --all-targets` checks the application and its tests for lint warnings.
- `cargo test` runs the test suite.
