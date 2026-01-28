use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tempfile::NamedTempFile;

use crate::types::{StartupEntry, StartupSource};

pub(crate) fn parse_desktop_file(path: &Path, source: StartupSource) -> Result<StartupEntry> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading desktop file {path:?}"))?;

    let mut name = String::from("Unnamed");
    let mut command = String::new();
    let mut enabled = true;
    let mut extra = Vec::new();
    let mut localized_names = Vec::new();
    let mut entry_comments = Vec::new();
    let mut preamble = Vec::new();
    let mut other_groups: Vec<Vec<String>> = Vec::new();

    let mut current_group: Option<String> = None;
    let mut group_lines: Vec<String> = Vec::new();
    let mut seen_entry_group = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Some(group) = current_group.take() {
                if group != "Desktop Entry" && !group_lines.is_empty() {
                    other_groups.push(group_lines.clone());
                }
                group_lines.clear();
            }
            current_group = Some(trimmed.trim_matches(&['[', ']'][..]).to_string());
            seen_entry_group |= current_group.as_deref() == Some("Desktop Entry");
            group_lines.push(line.to_string());
            continue;
        }

        match current_group.as_deref() {
            Some("Desktop Entry") => {
                if trimmed.starts_with('#') || trimmed.is_empty() {
                    entry_comments.push(line.to_string());
                    continue;
                }
                let mut parts = trimmed.splitn(2, '=');
                let key = parts.next().unwrap_or_default().trim();
                let value = parts.next().unwrap_or_default().trim();
                if key.starts_with("Name[") && key.ends_with(']') {
                    let locale = key.trim_start_matches("Name[").trim_end_matches(']');
                    localized_names.push((locale.to_string(), value.to_string()));
                } else {
                    match key {
                        "Name" => name = value.to_string(),
                        "Exec" => command = value.to_string(),
                        "Hidden" => enabled = value != "true",
                        "X-GNOME-Autostart-enabled" => enabled = value != "false",
                        _ => extra.push((key.to_string(), value.to_string())),
                    }
                }
                group_lines.push(line.to_string());
            }
            Some(_) => {
                group_lines.push(line.to_string());
            }
            None => {
                if !seen_entry_group {
                    preamble.push(line.to_string());
                }
            }
        }
    }

    if let Some(group) = current_group.take() {
        if group != "Desktop Entry" && !group_lines.is_empty() {
            other_groups.push(group_lines);
        }
    }

    Ok(StartupEntry {
        name,
        command,
        enabled,
        source,
        path: Some(path.to_path_buf()),
        extra,
        localized_names,
        entry_comments,
        preamble,
        other_groups,
    })
}

pub(crate) fn write_desktop_entry(entry: &StartupEntry, path: &Path) -> Result<()> {
    let mut dir = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    if dir.as_os_str().is_empty() {
        dir = PathBuf::from(".");
    }
    fs::create_dir_all(&dir).with_context(|| format!("Creating dir {:?}", dir))?;
    let mut tmp = NamedTempFile::new_in(&dir).with_context(|| format!("Creating temp file in {:?}", dir))?;
    let tmp_path = tmp.path().to_path_buf();
    let file = tmp.as_file_mut();
    let mut lines = Vec::new();
    lines.extend(entry.preamble.clone());
    if entry.preamble.last().map(|s| !s.is_empty()).unwrap_or(false) {
        lines.push(String::new());
    }

    lines.push("[Desktop Entry]".to_string());
    lines.extend(entry.entry_comments.clone());
    lines.push("Type=Application".to_string());
    lines.push(format!("Name={}", entry.name));
    for (locale, value) in entry.localized_names.iter() {
        lines.push(format!("Name[{locale}]={value}"));
    }
    lines.push(format!("Exec={}", entry.command));
    lines.push(format!(
        "X-GNOME-Autostart-enabled={}",
        if entry.enabled { "true" } else { "false" }
    ));
    lines.push(format!(
        "Hidden={}",
        if entry.enabled { "false" } else { "true" }
    ));
    let known = ["Name", "Exec", "Hidden", "X-GNOME-Autostart-enabled", "Type"];
    for (k, v) in entry.extra.iter() {
        if known.contains(&k.as_str()) || k.starts_with("Name[") {
            continue;
        }
        lines.push(format!("{k}={v}"));
    }

    if !entry.other_groups.is_empty() && !lines.last().map(|s| s.is_empty()).unwrap_or(true) {
        lines.push(String::new());
    }
    for (i, group) in entry.other_groups.iter().enumerate() {
        lines.extend(group.clone());
        if i + 1 != entry.other_groups.len() && !group.last().map(|s| s.is_empty()).unwrap_or(true) {
            lines.push(String::new());
        }
    }

    let content = if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.join("\n")
    } else {
        lines.join("\n") + "\n"
    };
    file.write_all(content.as_bytes())
        .with_context(|| format!("Writing {:?}", tmp_path))?;
    let _ = file.sync_all();
    tmp.persist(path)
        .with_context(|| format!("Replacing {:?}", path))?;
    Ok(())
}

pub(crate) fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c.is_whitespace() || c == '-' || c == '_' {
            if !out.ends_with('-') {
                out.push('-');
            }
        }
    }
    if out.is_empty() {
        "entry".into()
    } else {
        out
    }
}

pub(crate) fn source_label(source: &StartupSource) -> &'static str {
    match source {
        StartupSource::UserAutostart => "user",
        StartupSource::SystemAutostart => "system",
        StartupSource::ShellProfile => "shell",
        StartupSource::Unknown => "unknown",
    }
}
