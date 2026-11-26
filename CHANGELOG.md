# Changelog

## Unreleased

- Documented the initial Arch Linux package management TUI scope, including installed-package filters, repository and AUR search and transactions, Neovim-style navigation, and persistent appearance and package settings.
- Defined the first implementation milestones in a local MVP plan.
- Added the TUI shell with Installed, Search, and Settings views, Neovim-style navigation, and a keybinding help overlay.
- Added live installed-package filtering and background searches across configured repositories and the AUR.
- Added confirmed installs, removals, and full upgrades with interactive `pacman` and `yay` prompts.
- Added updates for selected packages and full upgrades with temporary exclusions, with package marks in Installed.
- Added editable TOML settings for themes, layout, and AUR support, plus a dedicated Kitty launch for font settings.
- Added searchable system font and theme pickers, including all 15 DWM Titus palettes and live theme previews.
- Added Flatpak install, search, update, and removal through configured installations.
- Added managed AppImage imports from local files or HTTPS, with updates from saved sources, replacement, and removal.
- Added persistent switches for Flatpak and AppImages support.
