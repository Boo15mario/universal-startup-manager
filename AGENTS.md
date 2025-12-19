# Repository Guidelines

## Project Structure & Module Organization
- `src/`: GTK4 Rust application code; `main.rs` contains UI, data model, `.desktop` parsing/writing, and dialogs.
- `README.md`: usage, dependencies, and distro-specific install notes.
- `AGENTS.md`: contributor guidance (this file).
- `Cargo.toml`: dependencies; `tempfile` used for safe writes and tests.
- Tests live inline in `src/main.rs` under `#[cfg(test)]`.

## Build, Test, and Development Commands
- `cargo build` / `cargo build --release`: compile in debug/release.
- `cargo run`: launch the GTK4 app locally.
- `cargo test`: run unit tests (parsing, filtering/sorting, slugify, `.desktop` roundtrips).
- Packaging: RPM spec builds offline from vendored crates; keep `vendor/` and `.cargo/config.toml` in sync with `Cargo.lock`, and keep `Cargo.lock` current for `--locked` builds. Tag releases (e.g., `v1.0.1`) before Copr builds.

## Coding Style & Naming Conventions
- Rust 2024 edition; prefer explicit types and early `?` returns with `anyhow::Result`.
- Keep UI wiring in `main.rs` organized by helper functions (e.g., `show_*_dialog`, `rebuild_list`).
- Use snake_case for functions/variables, UpperCamelCase for types/enums.
- Preserve `.desktop` fields when editing; avoid discarding unknown keys or localized names.

## Testing Guidelines
- Add new unit tests in `#[cfg(test)]` at the bottom of `src/main.rs`.
- Test parsing edge cases (comments, localized `Name[xx]`, extra keys) and UI logic helpers (`apply_filter`, `sort_indices`, `slugify`).
- Run `cargo test` before submitting changes.

## Commit & Pull Request Guidelines
- Use clear, imperative commit messages (e.g., “Add filter dialog”, “Preserve localized names on write”).
- Describe the change scope and any UI impacts; include before/after notes or screenshots for visible changes.
- Link related issues when applicable. Keep diffs focused; avoid unrelated formatting.

## Security & Configuration Tips
- No network calls; app operates on local files under `~/.config/autostart` and `/etc/xdg/autostart`.
- Writes use temp+rename; avoid shortcuts that bypass safe writes.
- System autostart entries are read-only by design—retain that guardrail.

## Agent-Specific Instructions
- Do not remove preserved `.desktop` fields (extra keys, localized names, comments, other groups).
- Keep accessibility affordances: labeled inputs, list accessible roles, empty-state announcement.
- Filters and sorting should remain client-side and non-destructive to stored data.
