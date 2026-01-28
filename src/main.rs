//! Universal Startup Manager — GTK4 scaffold for managing per-user autostart entries.
//! Loads XDG autostart `.desktop` files, lets you add user entries, toggle enablement,
//! and delete user-owned entries. System entries are read-only.

mod autostart;
mod desktop;
mod types;
mod ui;

use anyhow::Result;
use gtk4::prelude::*;
use gtk4::Application;

use crate::ui::build_ui;

fn main() -> Result<()> {
    let app = Application::builder()
        .application_id("com.example.universal-startup-manager")
        .build();

    app.connect_activate(|app| {
        if let Err(err) = build_ui(app) {
            eprintln!("Failed to build UI: {err:?}");
        }
    });

    app.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::read_to_string;

    use tempfile::tempdir;

    use crate::autostart::os_release_content_has_id;
    use crate::desktop::{parse_desktop_file, slugify, write_desktop_entry};
    use crate::types::{FilterState, SortKey, StartupEntry, StartupSource};
    use crate::ui::{apply_filter, sort_indices};

    fn entry(name: &str, command: &str, enabled: bool, source: StartupSource) -> StartupEntry {
        StartupEntry {
            name: name.to_string(),
            command: command.to_string(),
            enabled,
            source,
            path: None,
            extra: Vec::new(),
            localized_names: Vec::new(),
            entry_comments: Vec::new(),
            preamble: Vec::new(),
            other_groups: Vec::new(),
        }
    }

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("My App"), "my-app");
        assert_eq!(slugify("App_123"), "app-123");
        assert_eq!(slugify("$$$"), "entry");
    }

    #[test]
    fn filter_and_sort() {
        let entries = vec![
            entry("B", "/bin/true", true, StartupSource::UserAutostart),
            entry("A", "/bin/false", false, StartupSource::SystemAutostart),
            entry("C", "/bin/echo", true, StartupSource::UserAutostart),
        ];
        let filter = FilterState {
            show_enabled: true,
            show_disabled: false,
            show_user: true,
            show_system: true,
        };
        let filtered = apply_filter(&entries, &filter);
        assert_eq!(filtered.len(), 2);
        let sorted = sort_indices(&entries, filtered, SortKey::NameAsc);
        let names: Vec<_> = sorted.iter().map(|i| entries[*i].name.as_str()).collect();
        assert_eq!(names, vec!["B", "C"]);
        let sorted_status = sort_indices(&entries, vec![0, 1, 2], SortKey::StatusEnabledFirst);
        assert_eq!(sorted_status[0], 0); // enabled first
    }

    #[test]
    fn filter_combined_user_enabled() {
        let entries = vec![
            entry("UserEnabled", "/bin/true", true, StartupSource::UserAutostart),
            entry("UserDisabled", "/bin/true", false, StartupSource::UserAutostart),
            entry("SystemEnabled", "/bin/true", true, StartupSource::SystemAutostart),
        ];
        let filter = FilterState {
            show_enabled: true,
            show_disabled: false,
            show_user: true,
            show_system: false,
        };
        let filtered = apply_filter(&entries, &filter);
        assert_eq!(filtered.len(), 1);
        assert_eq!(entries[filtered[0]].name, "UserEnabled");
    }

    #[test]
    fn sort_localized_names_uses_base_name() {
        let mut a = entry("Äpple", "/bin/true", true, StartupSource::UserAutostart);
        a.localized_names.push(("de".into(), "Äpfel".into()));
        let b = entry("Banana", "/bin/true", true, StartupSource::UserAutostart);
        let indices = vec![0usize, 1usize];
        let sorted = sort_indices(&vec![a, b], indices, SortKey::NameAsc);
        // ASCII compare puts Banana before Äpple; ensure stable deterministic ordering
        assert_eq!(sorted, vec![1, 0]);
    }

    #[test]
    fn parse_write_preserves_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.desktop");
        let content = "\
# Preamble comment

[Desktop Entry]
# entry comment
Type=Application
Name=Sample
Name[de]=Beispiel
Exec=/bin/true
X-GNOME-Autostart-enabled=true
Hidden=false
X-Test=1

[Other]
Foo=Bar
";
        std::fs::write(&path, content).unwrap();
        let mut entry = parse_desktop_file(&path, StartupSource::UserAutostart).unwrap();
        assert_eq!(entry.name, "Sample");
        assert_eq!(entry.localized_names.len(), 1);
        assert_eq!(
            entry
                .extra
                .iter()
                .find(|(k, _)| k == "X-Test")
                .map(|(_, v)| v.as_str()),
            Some("1")
        );
        // Modify and write back
        entry.name = "Sample2".into();
        entry.command = "/bin/echo hi".into();
        write_desktop_entry(&entry, &path).unwrap();
        let written = read_to_string(&path).unwrap();
        assert!(written.contains("Name=Sample2"));
        assert!(written.contains("Name[de]=Beispiel"));
        assert!(written.contains("X-Test=1"));
        assert!(written.contains("[Other]"));
        assert!(written.contains("Foo=Bar"));
    }

    #[test]
    fn parse_ignores_non_entry_groups_for_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.desktop");
        let content = "\
[NotDesktop]
Name=ShouldNotUse

[Desktop Entry]
Name=Good
Exec=/bin/true
X-GNOME-Autostart-enabled=true
Hidden=false
";
        std::fs::write(&path, content).unwrap();
        let entry = parse_desktop_file(&path, StartupSource::UserAutostart).unwrap();
        assert_eq!(entry.name, "Good");
        assert_eq!(entry.command, "/bin/true");
    }

    #[test]
    fn parse_preserves_duplicate_unknown_keys_order() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.desktop");
        let content = "\
[Desktop Entry]
Name=Sample
Exec=/bin/true
X-GNOME-Autostart-enabled=true
Hidden=false
X-Test=1
X-Test=2
";
        std::fs::write(&path, content).unwrap();
        let entry = parse_desktop_file(&path, StartupSource::UserAutostart).unwrap();
        assert_eq!(entry.extra.iter().filter(|(k, _)| k == "X-Test").count(), 2);
        // Writing back should keep last value, but preserve order of extras
        write_desktop_entry(&entry, &path).unwrap();
        let written = read_to_string(&path).unwrap();
        assert!(written.contains("X-Test=1"));
        assert!(written.contains("X-Test=2"));
    }

    #[test]
    fn parse_preserves_entry_comments_and_preamble() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.desktop");
        let content = "\
# Preamble line 1

[Desktop Entry]
# comment inside
Name=Foo
Exec=/bin/true
Hidden=false
X-GNOME-Autostart-enabled=true
";
        std::fs::write(&path, content).unwrap();
        let entry = parse_desktop_file(&path, StartupSource::UserAutostart).unwrap();
        assert!(entry.preamble.iter().any(|l| l.contains("Preamble line 1")));
        assert!(entry.entry_comments.iter().any(|l| l.contains("comment inside")));
        write_desktop_entry(&entry, &path).unwrap();
        let written = read_to_string(&path).unwrap();
        assert!(written.contains("Preamble line 1"));
        assert!(written.contains("comment inside"));
    }

    #[test]
    fn localized_name_roundtrip_edit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.desktop");
        let content = "\
[Desktop Entry]
Name=Base
Name[fr]=BaseFr
Exec=/bin/true
X-GNOME-Autostart-enabled=true
Hidden=false
";
        std::fs::write(&path, content).unwrap();
        let mut entry = parse_desktop_file(&path, StartupSource::UserAutostart).unwrap();
        assert_eq!(entry.localized_names.len(), 1);
        entry.name = "NewBase".into();
        // Simulate editing localized name:
        entry.localized_names.retain(|(loc, _)| loc != "fr");
        entry.localized_names.push(("fr".into(), "Nouveau".into()));
        write_desktop_entry(&entry, &path).unwrap();
        let written = read_to_string(&path).unwrap();
        assert!(written.contains("Name=NewBase"));
        assert!(written.contains("Name[fr]=Nouveau"));
    }

    #[test]
    fn os_release_detects_nixos_id() {
        let content = "NAME=NixOS\nID=nixos\n";
        assert!(os_release_content_has_id(content, "nixos"));
    }

    #[test]
    fn os_release_rejects_other_ids() {
        let content = "NAME=Ubuntu\nID=ubuntu\nID_LIKE=debian\n";
        assert!(!os_release_content_has_id(content, "nixos"));
    }
}
