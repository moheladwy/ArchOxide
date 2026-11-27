# Changelog

## Unreleased

- Unified Search across Pacman, AUR, configured Flatpak remotes, and managed AppImages; added explicit source labels and source-aware installs, including direct AppImage imports.

- Documented the initial Arch Linux package management TUI scope, including installed-package filters, repository and AUR search and transactions, Neovim-style navigation, and persistent appearance and package settings.
- Recorded the implementation milestones in the [MVP plan](.github/plans/2026-11-26-archoxide-mvp.md).
- Added the TUI shell with Installed, Search, and Settings views, Neovim-style navigation, and a keybinding help overlay.
- Added live installed-package filtering and background searches across configured repositories and the AUR.
- Added confirmed installs, removals, and full upgrades with interactive `pacman` and `yay` prompts.
- Added updates for selected packages and full upgrades with temporary exclusions, with package marks in Installed.
- Added removal of marked Arch packages, Flatpaks, and managed AppImages with a confirmation that lists the selected targets.
- Added `install.sh` to build and install or update the release binary at `/usr/bin/archoxide`.
- Added editable TOML settings for themes, layout, and AUR support, plus a dedicated Kitty launch for font settings.
- Added searchable system font and theme pickers, including all 15 DWM Titus palettes and live theme previews.
- Added Flatpak install, search, update, and removal through configured installations.
- Added managed AppImage imports from local files or HTTPS, with updates from saved sources, replacement, and removal.
- Added persistent switches for Flatpak and AppImages support.
- Added an interactive terminal popup for package operations and live AppImage progress.
- Added `Ctrl+Home` and `Ctrl+End` to jump to the first and last rows in lists and appearance pickers.
- Extended the CI smoke test to answer a fixture package-manager prompt inside the popup and verify terminal restoration on quit.
