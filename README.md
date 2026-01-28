# Universal Startup Manager

GTK4 Rust app to manage autostart entries for the current user. It reads XDG autostart `.desktop` files, lists them, and lets you add/edit/delete/toggle user-owned entries. System entries are read-only. Filters (enabled/disabled, user/system) and accessibility-friendly dialogs are included.

## Requirements
- Rust toolchain (rustup + cargo)
- GTK4 development libraries (e.g., `libgtk-4-dev` on Debian/Ubuntu or `gtk4` via your distro)
- Build tools: `pkg-config`, a C compiler, and standard headers

### Install deps by distro
- Debian/Ubuntu: `sudo apt install build-essential pkg-config libgtk-4-dev rustc cargo`
- Fedora: `sudo dnf install @development-tools pkgconf-pkg-config gtk4-devel rust cargo`
- Arch: `sudo pacman -S base-devel pkgconf gtk4 rust` (cargo comes with rust)
- Gentoo: `sudo emerge --ask sys-devel/gcc virtual/pkgconfig x11-libs/gtk+:4 dev-lang/rust`
- NixOS: `nix-shell -p rustc cargo pkg-config gtk4` (or `nix shell nixpkgs#rustc nixpkgs#cargo nixpkgs#pkg-config nixpkgs#gtk4`)

## Run
```bash
cargo run
```

## Build
```bash
cargo build --release
```
The release binary will be at `target/release/universal-startup-manager`.

## Install (user local)
You can copy the release binary somewhere on your PATH, e.g.:
```bash
cargo build --release
install -m 755 target/release/universal-startup-manager ~/.local/bin/
```
Or with Cargo:
```bash
cargo install --path .
```
Make sure `~/.local/bin` is in your PATH.

## Install (NixOS)
Add a local build to `environment.systemPackages` or `home.packages`:
```nix
# configuration.nix or home.nix
{
  environment.systemPackages = [
    (pkgs.rustPlatform.buildRustPackage {
      pname = "universal-startup-manager";
      version = "git";
      src = /path/to/universal-startup-manager;
      cargoLock.lockFile = /path/to/universal-startup-manager/Cargo.lock;
      nativeBuildInputs = [ pkgs.pkg-config ];
      buildInputs = [ pkgs.gtk4 ];
    })
  ];
}
```
Home Manager variant:
```nix
# home.nix
{
  home.packages = [
    (pkgs.rustPlatform.buildRustPackage {
      pname = "universal-startup-manager";
      version = "git";
      src = /path/to/universal-startup-manager;
      cargoLock.lockFile = /path/to/universal-startup-manager/Cargo.lock;
      nativeBuildInputs = [ pkgs.pkg-config ];
      buildInputs = [ pkgs.gtk4 ];
    })
  ];
}
```
Flake example:
```nix
{
  description = "Universal Startup Manager";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
    in {
      packages.${system}.default = pkgs.rustPlatform.buildRustPackage {
        pname = "universal-startup-manager";
        version = "git";
        src = self;
        cargoLock.lockFile = ./Cargo.lock;
        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = [ pkgs.gtk4 ];
      };
    };
}
```
Usage:
```bash
nix build
./result/bin/universal-startup-manager
```
Or:
```bash
nix run
```

## Features
- Enumerates XDG autostart entries from `~/.config/autostart` and `/etc/xdg/autostart` (on NixOS also checks XDG config dirs like `/run/current-system/sw/etc/xdg/autostart`)
- Add, edit, delete, and toggle user-owned entries (system entries remain read-only)
- Filtering by enabled/disabled and user/system
- Sorting by name (asc/desc), status, or source (user-first/system-first) via dialog
- About dialog with version and short description
- Accessible dialogs and labels; empty-state announcement when no entries match filters
- Preserves extra `.desktop` keys, localized `Name[xx]`, comments, and other groups when rewriting files

## Notes
- Edits and additions write `.desktop` files to `~/.config/autostart` using temp+rename for safety. Renaming an entry deletes the old file to avoid duplicates.
- Filtering is client-side; use the Filter dialog (checkboxes) to control visibility.

## Next steps
- Keyboard shortcuts for common actions
- Preserve comments within `[Desktop Entry]` ordering more precisely; consider editing localized names
- Additional tests for localized edit flows and comment ordering
