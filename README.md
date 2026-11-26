# ArchOxide

ArchOxide is a keyboard-driven terminal UI for managing Arch Linux packages from configured `pacman` repositories and the Arch User Repository (AUR).

## Run the TUI

Run `cargo run --locked` from an interactive terminal on Arch Linux. Run ArchOxide as your regular user. It uses `pacman` to read configured repositories and `sudo` for repository transactions. Install `yay` to enable AUR search and builds. `cargo run --locked -- --kitty` opens a dedicated Kitty window with the saved font settings.

## MVP

- **Installed packages.** Filter the inventory by name, version, or availability. The Foreign label means a package is absent from the configured repository databases.
- **Search and package changes.** Search by name or description, install a result from its displayed source, remove an installed package, and update all packages, only selected packages, or all except selected packages. Package-manager prompts remain interactive, including build review when `yay` offers it. Repository features also work without `yay`.
- **Settings.** Change the theme, row density, details pane width, font family and size, and AUR enablement. Save in the TUI or edit the TOML file directly.

## Keys

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Switch views. `Alt+Tab` and `Alt+Shift+Tab` also work when forwarded by the terminal. |
| `j` / `k`, `gg` / `G`, `Ctrl+u` / `Ctrl+d` | Move, jump to the first or last row, or move half a page. |
| `h` / `l`, `Ctrl+h` / `Ctrl+l` | Focus the list or details pane when both are visible. |
| `/`, `a`, `R` | Filter Installed or open Search. In Installed, `a` cycles availability and `R` reloads the list. |
| `i`, `d` | Install the selected result or remove the highlighted installed package. |
| `Space u`, `u`, `U` | Update all packages, update selected packages, or update all except selected packages. Use `u` and `U` in Installed. |
| `x`, `X` | Toggle the highlighted package’s mark or clear all marks in Installed. |
| `Space e`, `Space f f`, `Space f t` | Open Installed, focus Search, or select Theme in Settings. |
| `Enter`, `s`, `r` | Edit the selected setting, save changes, or load defaults into the draft. |
| `?`, `Space q` | Show keybinding help or quit. |

In Installed, `u` and `U` use the marked packages, or the highlighted row when there are no marks. Marks survive filtering. Press `Enter` to confirm a transaction or `Esc` to cancel.

Updating selected packages uses the cached repository databases. Updating all except selected refreshes the databases and applies the exclusions for that run. Both modes preserve package-manager dependency checks. Arch supports full system upgrades, and selective upgrades can leave dependencies out of sync. [Pacman manual](https://man.archlinux.org/man/pacman.8), [Arch system maintenance](https://wiki.archlinux.org/title/System_maintenance#Partial_upgrades_are_unsupported)

## Configuration

Settings are saved to `$XDG_CONFIG_HOME/archoxide/config.toml`, with `~/.config/archoxide/config.toml` as the fallback. Omitted settings use these defaults.

```toml
[appearance]
theme = "dark"             # dark, light, or terminal
density = "comfortable"    # comfortable or compact
details_width_percent = 40 # 25–60
# font_family = "DejaVu Sans Mono"
# font_size_pt = 12.0       # 6–48

[packages]
aur_enabled = true
```

Use `archoxide --kitty` to apply the saved font family and size on launch. Regular launches inherit the current terminal’s font. [Ratatui font-size FAQ](https://ratatui.rs/faq/#can-you-change-font-size-in-a-terminal-using-ratatui), [Kitty command-line options](https://sw.kovidgoyal.net/kitty/invocation/)

## Development commands

- `cargo run` builds and runs the TUI.
- `cargo fmt --check` verifies formatting.
- `cargo clippy --all-targets` checks the application and its tests for lint warnings.
- `cargo test` runs the test suite.

## Contributing

Choose the issue form that matches your report when opening an issue. New PRs start with a general template. For a focused PR description, add `template=bug-fix.md`, `template=feature.md`, `template=chore.md`, or `template=docs.md` to your branch’s compare URL. [GitHub pull request query parameters](https://docs.github.com/en/pull-requests/reference/using-query-parameters-to-create-a-pull-request)
