# ArchOxide

ArchOxide is a keyboard-driven terminal UI for managing Arch Linux packages from configured `pacman` repositories and the Arch User Repository (AUR), plus Flatpak apps and AppImages.

## Install and run

Run `./install.sh` from the checkout as your regular user. It uses Cargo and Rust to build an optimized release and uses `sudo` to place the binary at `/usr/bin/archoxide`. Run the same script again to replace an existing binary with a build from your current checkout. Set `DESTDIR` to a writable staging directory to install under that directory without `sudo`.

Run `archoxide` from an interactive terminal on Arch Linux, or use `cargo run --locked` from the checkout. ArchOxide uses `pacman` to read configured repositories and `sudo` for repository transactions. Install `yay` for AUR support, `flatpak` for Flatpak apps, `curl` for HTTPS AppImages, and Fontconfig for the installed font picker. Run `archoxide --kitty` to open a dedicated Kitty window with the saved font settings.

### Arch package

The [PKGBUILD](PKGBUILD) builds `archoxide-git` from the upstream `main` branch and packages `/usr/bin/archoxide` for pacman. Run `makepkg -s` as a regular user to build the package after the source URL is publicly accessible. Use pacman to install and upgrade the resulting package if you choose this installation method.

To submit to the AUR, regenerate `.SRCINFO` with `makepkg --printsrcinfo > .SRCINFO`, then commit `PKGBUILD`, `.SRCINFO`, and `LICENSE` in the separate `archoxide-git` AUR repository. [AUR submission guidelines](https://wiki.archlinux.org/title/AUR_submission_guidelines)

## Package sources and settings

- **Installed packages.** Filter the inventory by name, version, or availability. The Foreign label means a package is absent from the configured repository databases.
- **Search and package changes.** Search Pacman repositories, AUR, configured Flatpak remotes, and managed AppImages together. Every result shows the package name on the left and the source type on the right. The result details show the repository or the Flatpak remote and installation scope. Install a result from its displayed source, remove selected packages, and update all packages, only selected packages, or all except selected packages. Package-manager prompts remain interactive, including build review when `yay` offers it. Repository features also work without `yay`.
- **Flatpak and AppImages.** Manage apps using configured Flatpak installations or import AppImages from local files and direct HTTPS downloads.
- **Apps.** Browse and open desktop launchers from your session’s XDG application directories, including Flatpak exports on its data path. Opening apps uses `gio` from the `glib2` package on Arch Linux.
- **Settings.** Preview 18 themes, including all 15 DWM Titus palettes. Choose an installed font family, set its size, adjust row density and details width, and toggle AUR, Flatpak, or AppImages support.

## Keys

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Switch views. `Alt+Tab` and `Alt+Shift+Tab` also work when forwarded by the terminal. |
| `j` / `k`, `Ctrl+u` / `Ctrl+d` | Move by a row or half a page. |
| `gg` / `G`, `Ctrl+Home` / `Ctrl+End` | Jump to the first or last row. |
| `h` / `l`, `Ctrl+h` / `Ctrl+l` | Return to the list or focus Details. On narrow terminals, Details takes the full width for packages. |
| `/`, `a`, `R` | Filter an installed list or edit the search query. In Installed, `a` cycles availability. `R` reloads inventories and Flatpak search results. |
| `i`, `d` | Install the selected result or remove the selected items in an installed list. |
| `Space u`, `u`, `U` | Update all apps in the current source, update selected apps, or update all except selected apps. Use `u` and `U` in an installed list. |
| `x`, `X` | Toggle the highlighted app’s mark or clear all marks in the current installed list. |
| `Space e`, `Space f f`, `Space f t` | Open Installed, focus Search, or select Theme in Settings. |
| `Space p`, `Space a`, `Space o` | Open Flatpak, AppImages, or Apps. |
| `f`, `Esc` | Search Flatpak remotes or return to installed Flatpaks from the results. |
| `i`, `e` in AppImages | Import an AppImage or replace the highlighted managed copy. |
| `Enter` | Open the selected desktop app in Apps or edit the selected setting in Settings. |
| `s`, `r` | In Settings, save changes or load defaults into the draft. |
| `?`, `Space q` | Show keybinding help or quit. |

## Package details

Highlight a package to load its full record in **Details**. Installed Arch packages use `pacman -Qi`, repository search results use `pacman -Si`, and AUR results use `yay -Si --aur`. Flatpak details come from the selected installation and remote. AppImages show their saved source and available local file information.

Press `l` to focus Details, then use `j`/`k` to scroll, `Ctrl+u`/`Ctrl+d` to move by half a page, or `gg`/`G` to jump to the start or end. Press `h` or `Esc` to return to the list.

## Package transactions

Mark items with `x` in Installed, Flatpak, or AppImages, then press `d` to remove those items. With no marks, `d` targets the highlighted row. Marks survive filtering, and the confirmation lists the targets. The `u` and `U` update shortcuts use the same selection rules. Press `Enter` to confirm a transaction or `Esc` to cancel.

Confirmed installs, removals, and updates open a terminal popup with live output. Answer package-manager prompts directly in the popup. AppImage operations report their progress in the same view. Use `Ctrl+C` to interrupt an interactive command and `Shift+PgUp` or `Shift+PgDn` to scroll while it runs. Once the operation finishes, use `PgUp` and `PgDn` to review output or `Enter` and `Esc` to close the popup.

For Arch packages, a selected upgrade uses the cached repository databases. Updating all except selected refreshes those databases and applies exclusions for that run. Both modes preserve package-manager dependency checks. Arch supports full system upgrades, and selective upgrades can leave dependencies out of sync. [Pacman manual](https://man.archlinux.org/man/pacman.8), [Arch system maintenance](https://wiki.archlinux.org/title/System_maintenance#Partial_upgrades_are_unsupported)

## Flatpak and AppImages

The unified **Search** tab respects the AUR, Flatpak, and AppImage switches in Settings. AppImage name searches find installed managed copies. Submit an existing local `.AppImage` path or direct HTTPS URL to find an importable AppImage. Press `i` and confirm to import it.

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

## License

ArchOxide is released under the [MIT License](LICENSE).
