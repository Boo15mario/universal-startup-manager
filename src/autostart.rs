use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::desktop::{parse_desktop_file, slugify, write_desktop_entry};
use crate::types::{StartupEntry, StartupSource};

pub(crate) fn load_entries() -> Result<Vec<StartupEntry>> {
    let mut entries = Vec::new();
    entries.extend(load_autostart_dir(
        user_autostart_dir().as_ref(),
        StartupSource::UserAutostart,
    )?);
    for dir in system_autostart_dirs() {
        entries.extend(load_autostart_dir(
            dir.as_ref(),
            StartupSource::SystemAutostart,
        )?);
    }
    Ok(entries)
}

pub(crate) fn user_autostart_dir() -> PathBuf {
    let mut base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
    base.push("autostart");
    base
}

fn system_autostart_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen = Vec::new();
    push_system_dir(&mut dirs, &mut seen, PathBuf::from("/etc/xdg/autostart"));
    if is_nixos() {
        for dir in nixos_system_autostart_dirs() {
            push_system_dir(&mut dirs, &mut seen, dir);
        }
    }
    dirs
}

fn push_system_dir(dirs: &mut Vec<PathBuf>, seen: &mut Vec<PathBuf>, candidate: PathBuf) {
    if candidate.as_os_str().is_empty() {
        return;
    }
    let normalized = fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
    if seen.iter().any(|path| path == &normalized) {
        return;
    }
    seen.push(normalized);
    dirs.push(candidate);
}

fn is_nixos() -> bool {
    let paths = ["/etc/os-release", "/usr/lib/os-release"];
    for path in paths {
        if let Ok(content) = fs::read_to_string(path) {
            if os_release_content_has_id(&content, "nixos") {
                return true;
            }
        }
    }
    false
}

pub(crate) fn os_release_content_has_id(content: &str, target: &str) -> bool {
    content.lines().any(|line| {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("ID=") {
            let value = value.trim_matches('"');
            return value == target;
        }
        false
    })
}

fn nixos_system_autostart_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(config_dirs) = env::var("XDG_CONFIG_DIRS") {
        for base in config_dirs.split(':') {
            let base = base.trim();
            if base.is_empty() {
                continue;
            }
            let mut path = PathBuf::from(base);
            path.push("autostart");
            dirs.push(path);
        }
    }
    dirs.push(PathBuf::from("/run/current-system/sw/etc/xdg/autostart"));
    dirs.push(PathBuf::from(
        "/nix/var/nix/profiles/default/etc/xdg/autostart",
    ));
    dirs
}

fn load_autostart_dir(dir: &Path, source: StartupSource) -> Result<Vec<StartupEntry>> {
    let mut entries = Vec::new();
    if !dir.exists() {
        return Ok(entries);
    }

    for entry in fs::read_dir(dir).with_context(|| format!("reading dir {dir:?}"))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
            continue;
        }
        match parse_desktop_file(&path, source.clone()) {
            Ok(item) => entries.push(item),
            Err(err) => eprintln!("Skipping {:?}: {err:?}", path),
        }
    }
    Ok(entries)
}

pub(crate) fn edit_user_entry(
    original: &StartupEntry,
    new_name: &str,
    new_cmd: &str,
    original_path: Option<&PathBuf>,
) -> Result<()> {
    let mut updated = original.clone();
    updated.name = new_name.to_string();
    updated.command = new_cmd.to_string();
    let target_path = if let Some(p) = original_path {
        p.clone()
    } else {
        user_autostart_dir().join(format!("{}.desktop", slugify(new_name)))
    };
    let target_path = validate_user_entry_path(&target_path)?;
    write_desktop_entry(&updated, &target_path)?;
    // If slug/name changed, remove old file to avoid duplicates.
    if let Some(old_path) = original_path {
        if old_path != &target_path {
            if let Ok(old_path) = validate_user_entry_path(old_path) {
                let _ = fs::remove_file(old_path);
            }
        }
    }
    Ok(())
}

pub(crate) fn create_user_entry(name: &str, command: &str) -> Result<PathBuf> {
    if name.trim().is_empty() || command.trim().is_empty() {
        bail!("Name and command are required");
    }
    let dir = user_autostart_dir();
    fs::create_dir_all(&dir).with_context(|| format!("Creating dir {:?}", dir))?;
    let file_name = format!("{}.desktop", slugify(name));
    let path = dir.join(file_name);
    let path = validate_user_entry_path(&path)?;
    let entry = StartupEntry {
        name: name.to_string(),
        command: command.to_string(),
        enabled: true,
        source: StartupSource::UserAutostart,
        path: Some(path.clone()),
        extra: Vec::new(),
        localized_names: Vec::new(),
        entry_comments: Vec::new(),
        preamble: Vec::new(),
        other_groups: Vec::new(),
    };
    write_desktop_entry(&entry, &path)?;
    Ok(path)
}

pub(crate) fn is_user_owned_path(path: &Path) -> bool {
    let base = user_autostart_dir();
    let base_canon = match base.canonicalize() {
        Ok(path) => path,
        Err(_) => return false,
    };
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let parent_canon = match parent.canonicalize() {
        Ok(path) => path,
        Err(_) => return false,
    };
    if parent_canon != base_canon {
        return false;
    }
    match fs::symlink_metadata(path) {
        Ok(meta) => meta.is_file() && !meta.file_type().is_symlink(),
        Err(_) => false,
    }
}

pub(crate) fn validate_user_entry_path(path: &Path) -> Result<PathBuf> {
    let base = user_autostart_dir();
    let base_canon = base
        .canonicalize()
        .with_context(|| format!("Resolving {:?}", base))?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let parent_canon = parent
        .canonicalize()
        .with_context(|| format!("Resolving {:?}", parent))?;
    if parent_canon != base_canon {
        bail!("Entry path is outside user autostart dir");
    }
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            bail!("Refusing to operate on symlinked entry");
        }
        if !meta.is_file() {
            bail!("Entry path is not a regular file");
        }
    }
    Ok(path.to_path_buf())
}
