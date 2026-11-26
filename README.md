# ArchOxide

ArchOxide is a keyboard-driven terminal UI for managing Arch Linux packages from configured `pacman` repositories and the Arch User Repository (AUR), plus Flatpak apps and AppImages.

## Run the TUI

Run `cargo run --locked` from an interactive terminal on Arch Linux as your regular user. ArchOxide uses `pacman` to read configured repositories and `sudo` for repository transactions. Install `yay` for AUR support, `flatpak` for Flatpak apps, `curl` for HTTPS AppImages, and Fontconfig for the installed font picker. `cargo run --locked -- --kitty` opens a dedicated Kitty window with the saved font settings.

## Package sources and settings

- **Installed packages.** Filter the inventory by name, version, or availability. The Foreign label means a package is absent from the configured repository databases.
- **Search and package changes.** Search by name or description, install a result from its displayed source, remove selected packages, and update all packages, only selected packages, or all except selected packages. Package-manager prompts remain interactive, including build review when `yay` offers it. Repository features also work without `yay`.
- **Flatpak and AppImages.** Manage apps using configured Flatpak installations or import AppImages from local files and direct HTTPS downloads.
- **Settings.** Preview 18 themes, including all 15 DWM Titus palettes. Choose an installed font family, set its size, adjust row density and details width, and toggle AUR, Flatpak, or AppImages support.

## Keys

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Switch views. `Alt+Tab` and `Alt+Shift+Tab` also work when forwarded by the terminal. |
| `j` / `k`, `Ctrl+u` / `Ctrl+d` | Move by a row or half a page. |
| `gg` / `G`, `Ctrl+Home` / `Ctrl+End` | Jump to the first or last row. |
| `h` / `l`, `Ctrl+h` / `Ctrl+l` | Focus the list or details pane when both are visible. |
| `/`, `a`, `R` | Filter an installed list or edit the search query. In Installed, `a` cycles availability. `R` reloads inventories and Flatpak search results. |
| `i`, `d` | Install the selected result or remove the selected items in an installed list. |
| `Space u`, `u`, `U` | Update all apps in the current source, update selected apps, or update all except selected apps. Use `u` and `U` in an installed list. |
| `x`, `X` | Toggle the highlighted app’s mark or clear all marks in the current installed list. |
| `Space e`, `Space f f`, `Space f t` | Open Installed, focus Search, or select Theme in Settings. |
| `Space p`, `Space a` | Open Flatpak or AppImages. |
| `f`, `Esc` | Search Flatpak remotes or return to installed Flatpaks from the results. |
| `i`, `e` in AppImages | Import an AppImage or replace the highlighted managed copy. |
| `Enter`, `s`, `r` | Edit the selected setting, save changes, or load defaults into the draft. |
| `?`, `Space q` | Show keybinding help or quit. |

## Package transactions

Mark items with `x` in Installed, Flatpak, or AppImages, then press `d` to remove those items. With no marks, `d` targets the highlighted row. Marks survive filtering, and the confirmation lists the targets. The `u` and `U` update shortcuts use the same selection rules. Press `Enter` to confirm a transaction or `Esc` to cancel.

Confirmed installs, removals, and updates open a terminal popup with live output. Answer package-manager prompts directly in the popup. AppImage operations report their progress in the same view. Use `Ctrl+C` to interrupt an interactive command and `Shift+PgUp` or `Shift+PgDn` to scroll while it runs. Once the operation finishes, use `PgUp` and `PgDn` to review output or `Enter` and `Esc` to close the popup.

For Arch packages, a selected upgrade uses the cached repository databases. Updating all except selected refreshes those databases and applies exclusions for that run. Both modes preserve package-manager dependency checks. Arch supports full system upgrades, and selective upgrades can leave dependencies out of sync. [Pacman manual](https://man.archlinux.org/man/pacman.8), [Arch system maintenance](https://wiki.archlinux.org/title/System_maintenance#Partial_upgrades_are_unsupported)

## Flatpak and AppImages

- In **Flatpak**, press `/` to filter installed apps or `f` to search configured remotes. Press `i` on a result to install it in the displayed scope, or `Esc` to return to installed apps. Removals target each app’s full reference within its installation. [Flatpak command reference](https://docs.flatpak.org/en/latest/flatpak-command-reference.html#flatpak-uninstall)
- In **AppImages**, press `i` and enter a local file path or direct HTTPS download URL. ArchOxide stores managed copies under `$XDG_DATA_HOME/archoxide/appimages` (default `~/.local/share/archoxide/appimages`). Press `/` to filter managed files, `u` to refresh from the saved source, `e` to replace one from a new source, or `d` to remove selected managed copies. If a download link changes for each release, use `e` with the new link. [AppImage distribution guide](https://docs.appimage.org/packaging-guide/distribution.html)

## Configuration

Settings are saved to `$XDG_CONFIG_HOME/archoxide/config.toml`, with `~/.config/archoxide/config.toml` as the fallback. Omitted settings use these defaults.

```toml
[appearance]
theme = "dark"             # also supports DWM Titus IDs such as nord and catppuccin-latte
density = "comfortable"    # comfortable or compact
details_width_percent = 40 # 25–60
# font_family = "DejaVu Sans Mono"
# font_size_pt = 12.0       # 6–48

[packages]
aur_enabled = true
flatpak_enabled = true
appimage_enabled = true
```

In Settings, press `Enter` on **Theme** or **Font family** to open a picker, `/` to filter, and `Enter` to choose. Theme previews follow the highlighted choice. Press `s` to save. Use `archoxide --kitty` to apply the saved font family and size on launch. Regular launches inherit the current terminal’s font. [Ratatui font-size FAQ](https://ratatui.rs/faq/#can-you-change-font-size-in-a-terminal-using-ratatui), [Kitty command-line options](https://sw.kovidgoyal.net/kitty/invocation/)

Press `Enter` on **Flatpak support** or **AppImages support** to toggle that source. Save to apply the change and hide disabled sources from the tab bar.

## Development commands

- `cargo run --locked` builds and runs the TUI in an interactive terminal.
- `cargo build --locked` builds the binary using the checked-in dependency versions.
- `cargo fmt --check` verifies formatting.
- `cargo clippy --locked --all-targets -- -D warnings` checks the application and its tests for lint warnings.
- `cargo test --locked` runs the test suite.

Unit test files live under `tests/unit/` and mirror their source modules. They run with `cargo test --locked` and can access private code through their parent modules.

The [CI workflow](.github/workflows/ci.yml) runs on pull requests and on pushes to `main` that touch source files, test files, the workflow, or Cargo metadata. Its smoke test drives a fixture transaction through the terminal popup.

## Contributing

Choose the [issue form](.github/ISSUE_TEMPLATE) that matches your report. New PRs start with the [general template](.github/pull_request_template.md). For a focused PR description, add `template=bug-fix.md`, `template=feature.md`, `template=chore.md`, or `template=docs.md` to your branch’s compare URL. [GitHub pull request query parameters](https://docs.github.com/en/pull-requests/reference/using-query-parameters-to-create-a-pull-request)

The [MVP plan](.github/plans/2026-11-26-archoxide-mvp.md) records the source layout, completed milestones, and verification checks.
