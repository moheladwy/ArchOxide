# ArchOxide MVP Implementation Plan

ArchOxide’s MVP established installed-package browsing, repository and AUR search, interactive transactions, and persistent settings. Subsequent milestones added Flatpak and AppImages, appearance pickers, and an embedded transaction terminal. The [README](../../README.md) covers everyday usage.

ArchOxide uses Rust 2024, Ratatui and Crossterm for rendering and input, `tui-term` to render the transaction terminal, and Serde with TOML for settings. Typed transactions invoke package tools through `std::process::Command`. Run ArchOxide as the regular user and let package tools request elevation where needed.

## File layout

| File | Responsibility |
| --- | --- |
| `src/main.rs`, `Cargo.toml`, `Cargo.lock` | Startup, module wiring, dependencies, and locked versions |
| `src/app.rs`, `src/app/source_actions.rs`, `src/input.rs` | View state, selection, key handling, and source actions |
| `src/ui.rs`, `src/terminal.rs` | Ratatui rendering, terminal lifecycle, and event dispatch |
| `src/transaction_terminal.rs`, `src/pty.rs` | Interactive transaction popup, input translation, and Linux child-terminal I/O |
| `src/packages.rs`, `src/flatpak.rs`, `src/appimage.rs` | Package records, command construction, output parsing, and managed AppImage storage |
| `src/reads.rs`, `src/source_views.rs` | Background inventory and search results, source filtering, and marks |
| `src/config.rs`, `src/fonts.rs`, `src/themes.rs`, `src/picker.rs` | Persistent settings, Kitty options, Fontconfig discovery, theme palettes, and choice lists |

## 1. TUI shell and navigation

- [x] Add a Crossterm-backed Ratatui event loop with Installed, Search, and Settings views, a context-sensitive key hint bar, and a keybinding help overlay. Start in Installed. Restore terminal mode, screen, and cursor on exit and errors, including a partial initialization failure.
- [x] Define application state for active view, selection, focus, input mode, and sequence prefixes. Route key events through `App::handle_key` in `src/app.rs` with bindings defined in `src/input.rs`. Use a testable clock or explicit deadline for sequence timeout and a 700 ms default.
- [x] Implement the bindings below. In text entry, ordinary characters edit the field. `Enter` submits and `Esc` clears the pending sequence or closes the innermost prompt or overlay.
- [x] Test keyboard sequences, selection clamping on empty or shortened lists, timeout and cancellation, and text entry with characters that are shortcuts in normal mode. Render a narrow and a wide layout with Ratatui’s test backend. Smoke-test resize and terminal restoration from a real terminal.

| Keys | Action | Basis |
| --- | --- | --- |
| `j` / `k`, `↓` / `↑` | Move through rows | Vim movement |
| `gg` / `G`, `Ctrl+Home` / `Ctrl+End` | Jump to the first or last row | Vim and terminal navigation |
| `Ctrl+u` / `Ctrl+d` | Move half a visible page | Vim navigation |
| `h` / `l`, `Ctrl+h` / `Ctrl+l` | Focus the list or details pane when both are visible | AstroNvim split navigation, with single-key fallbacks |
| `Alt+Tab` / `Alt+Shift+Tab`, `Tab` / `Shift+Tab` | Cycle views forward or backward | Custom AstroNvim buffer shortcuts, with plain Tab fallbacks |
| `/` | Filter Installed or open the query field in Search | Vim search |
| `Space e` / `Space f f` / `Space f t` | Open Installed, focus package search, or open the theme setting | AstroNvim explorer, file finder, and theme picker |
| `Space p` / `Space a` | Open Flatpak or AppImages | Package source shortcuts |
| `Space q`, `?` | Quit or show keybinding help | AstroNvim quit and a TUI help shortcut |

The local sources are `~/.config/nvim/lua/lazy_setup.lua` for the Space leader, `~/.config/nvim/lua/plugins/astrocore.lua` for the Alt+Tab pair, and the installed AstroNvim core, Neo-tree, and Snacks mappings for the split, explorer, and finder keys. ArchOxide ships with these defaults and runs independently of Neovim.

## 2. Installed packages and repository search

- [x] Define `InstalledPackage` with name, version, and availability (`Repository`, `Foreign`, or `Unknown`) and `SearchResult` with name, version, source, and description. Use `(source, name)` to identify a search result and the package name to identify an installed entry.
- [x] Define read operations for listing installed packages and searching a single source in `src/packages.rs`. Use `pacman -Q` for names and versions and `pacman -Qqm` for foreign names. Label a package Foreign when it is absent from the configured sync databases. If the foreign query fails, preserve the installed rows and show Unknown with the error. Treat exit 1 with empty output and no diagnostic as an empty result for the pacman queries where that is the documented or observed no-match behavior.
- [x] Search configured repositories with `pacman -Ss --color never -- <query>` using a regex-escaped literal. Search AUR with `yay -Ssa --color never --doublelineresults -- <query>` when available and enabled. Skip empty queries. Set `LC_ALL=C` for reads, parse stdout and diagnostics separately, and keep untrusted descriptions as display text without terminal control characters. Verify no-match and error handling against each tool’s exit status and output.
- [x] Filter Installed as the user types, matching name and version case-insensitively. Add an availability selector. Start Search on `Enter`. Keep repository and AUR results distinct, include the source and installed version where available, and render loading, empty, and per-source error states. If `yay` is missing, show AUR as unavailable while the repository search remains usable.
- [x] Run read commands off the UI thread with at most one search in flight per source. Retain the latest pending query and apply only results matching the currently submitted query’s request ID. Preserve a selected row by identity when it is still visible.
- [x] Test realistic command output with fixtures, including duplicate names from different sources, an empty foreign set, empty search results, plain-text metacharacters, and a failed AUR search with repository results still visible. Test that a late result from an earlier query leaves the current results intact.

## 3. Install, remove, and upgrade

| Action | Command |
| --- | --- |
| Install selected repository package | `sudo pacman -S --needed -- <repo>/<name>` |
| Install selected AUR package | `yay -S --needed -- aur/<name>` |
| Remove selected installed package | `sudo pacman -R -- <name>` |
| Full upgrade with AUR enabled and `yay` available | `yay -Syu` |
| Full repository upgrade | `sudo pacman -Syu` |

- [x] Define typed install, removal, and full upgrade operations in `src/packages.rs`. Build each invocation as a program with separate arguments using validated targets from selected rows. Repository installs use the cached databases. A database refresh is part of an explicitly confirmed full upgrade. [pacman manual](https://man.archlinux.org/man/pacman.8.en), [Arch system maintenance](https://wiki.archlinux.org/title/System_maintenance#Partial_upgrades_are_unsupported), [yay manual](https://github.com/Jguer/yay/blob/next/doc/yay.8)
- [x] Bind `i` in Search to install the selected result, `d` in Installed to remove the selected package, and `Space u` to open full upgrade confirmation. Show the exact selected package and source for install or removal and the enabled sources for an upgrade. Start only after confirmation and allow `Esc` to cancel. Return a clear error if the helper or elevation command required for the confirmed operation is missing.
- [x] Add selected upgrades and upgrades with temporary exclusions. Marks survive filtering, and the highlighted row serves as the target when no rows are marked.
- [x] Open the in-app terminal popup after confirmation. `sudo`, `pacman`, `yay`, and Flatpak use the popup’s pseudo-terminal for input and output, including password prompts and AUR build-file review. Forward resize and `Ctrl+C` to the running command. Retain output after completion and refresh the affected inventory, including after a failure that changes package state.
- [x] Test exact command arguments, cancellation, startup failures, nonzero exit, input forwarding, buffered output, and terminal restoration. The CI smoke test answers a fixture prompt and verifies that the transaction stays inside the TUI.

## 4. Persistent settings and Kitty fonts

Use `$XDG_CONFIG_HOME/archoxide/config.toml`, falling back to `~/.config/archoxide/config.toml`. Omitted fields use defaults.

| TOML key | Values and default |
| --- | --- |
| `appearance.theme` | 18 presets, including `dark`, `light`, `terminal`, and 15 DWM Titus palettes. Default `dark`. |
| `appearance.density` | `compact` or `comfortable`. Default `comfortable`. Density changes padding and row spacing. |
| `appearance.details_width_percent` | Integer from 25 to 60. Default `40`. The UI uses a single-pane layout in narrow terminals. |
| `appearance.font_family` | Optional installed monospace font family. Omit to inherit Kitty’s setting. |
| `appearance.font_size_pt` | Optional size from 6 to 48 points. Omit to inherit Kitty’s setting. |
| `packages.aur_enabled` | Boolean. Default `true`. AUR actions also require an available `yay` binary. |
| `packages.flatpak_enabled` | Boolean. Default `true`. Controls access to the Flatpak view. |
| `packages.appimage_enabled` | Boolean. Default `true`. Controls access to the AppImages view. |

- [x] Define `Config::load(path)` and `Config::save(path)` in `src/config.rs`, plus a config-path resolver. Validate bounds and font input, keep a malformed config available for correction while showing the startup error, and write changes atomically in the config directory. Report save errors in Settings.
- [x] Add keyboard editing, Save, and Reset to Defaults to Settings. Reset fills the form with defaults for review before saving. Apply a successfully saved theme, density, pane width, and AUR setting immediately. When AUR enablement changes, invalidate pending AUR reads and refresh the active search. Keep focus usable when the terminal becomes too narrow for two panes. Explain that font changes apply on the next `--kitty` launch.
- [x] Add `archoxide --kitty` to open a dedicated Kitty process with `--override` arguments for the configured font family and size and launch the same binary in normal TUI mode. Pass a family with spaces as one argument. Report a missing Kitty executable clearly and support ordinary launches with the current terminal’s font. [Ratatui font-size FAQ](https://ratatui.rs/faq/#can-you-change-font-size-in-a-terminal-using-ratatui), [Kitty command-line options](https://sw.kovidgoyal.net/kitty/invocation/#cmdoption-kitty-o), [Kitty font settings](https://sw.kovidgoyal.net/kitty/conf/#fonts)
- [x] Test default loading, partial and invalid TOML, save and reload, failed save preserving the previous file, and the generated Kitty arguments. Verify that a Kitty launch uses the selected font and size and that changing density or resizing affects the layout in both launch modes.
- [x] Add searchable theme and font pickers. Preview themes as the highlight moves, discover installed font families through Fontconfig, and use `Ctrl+Home` or `Ctrl+End` to jump in either picker.

## 5. Flatpak and AppImages

- [x] Browse installed Flatpaks and search configured remotes. Install and remove apps by exact reference and installation. Support full and scoped updates with Flatpak’s native prompts.
- [x] Manage AppImages imported from local files or direct HTTPS downloads. Validate and stage each copy, remember its source for refreshes, allow explicit replacement, and display progress in the transaction popup.
- [x] Save the Flatpak and AppImages switches independently. Disabled sources are removed from tab navigation, and enabling one again allows the inventory to reload.

## 6. Verification

- [x] Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked`, and `cargo test --locked`. The [CI workflow](../workflows/ci.yml) also exercises the interactive popup from a pseudo-terminal.
- [x] In a disposable Arch Linux environment with a TTY and `yay` absent, browse and filter installed packages, search configured repositories, install and remove a known package, and run a full repository upgrade. With `yay` present, search the AUR, install a test package, and verify it appears in Installed. Run the combined upgrade with an AUR update available, verify the installed version changes, and remove the test package. Verify cancel returns to the TUI without starting a transaction.
- [x] Check navigation and settings in a regular terminal and through `--kitty`. Restart after saving and verify the settings persist. Confirm that a theme change, a density change, and a narrow terminal each produce a readable layout.
- [x] Verify Flatpak install, update, and removal using an isolated installation. Verify AppImage import, refresh, replacement, and removal using temporary copies.
- [x] Update `README.md` with the implemented launch commands, keybindings, settings path, and package-tool requirements. Record delivered behavior in `CHANGELOG.md`.
