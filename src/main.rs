//! Universal Startup Manager — GTK4 scaffold for managing per-user autostart entries.
//! Loads XDG autostart `.desktop` files, lets you add user entries, toggle enablement,
//! and delete user-owned entries. System entries are read-only.

use std::cell::{Cell, RefCell};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use gtk4::prelude::*;
use gtk4::{
    AccessibleRole, Application, ApplicationWindow, Box as GtkBox, Button, CheckButton, Dialog,
    Entry, HeaderBar, Label, ListBox, ListBoxRow, Orientation, ResponseType, ScrolledWindow,
    SelectionMode,
};
use tempfile::NamedTempFile;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum StartupSource {
    UserAutostart,
    SystemAutostart,
    ShellProfile,
    Unknown,
}

#[derive(Debug, Clone)]
struct StartupEntry {
    name: String,
    command: String,
    enabled: bool,
    source: StartupSource,
    path: Option<PathBuf>,
    extra: Vec<(String, String)>, // preserve additional keys in Desktop Entry group
    localized_names: Vec<(String, String)>, // locale -> name
    entry_comments: Vec<String>,            // comments/blank lines inside Desktop Entry
    preamble: Vec<String>,                  // lines before first group
    other_groups: Vec<Vec<String>>,         // raw lines for non-Desktop Entry groups
}

#[derive(Clone)]
struct AppState {
    entries: Rc<RefCell<Vec<StartupEntry>>>,
    visible_indices: Rc<RefCell<Vec<usize>>>,
    filter: Rc<RefCell<FilterState>>,
    sort: Rc<Cell<SortKey>>,
    selected: Rc<Cell<Option<usize>>>,
    list_box: ListBox,
    detail_name: Label,
    detail_command: Label,
    detail_source: Label,
    detail_status: Label,
    status_bar: Label,
    toggle_button: Button,
    delete_button: Button,
    edit_button: Button,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FilterState {
    show_enabled: bool,
    show_disabled: bool,
    show_user: bool,
    show_system: bool,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            show_enabled: true,
            show_disabled: true,
            show_user: true,
            show_system: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortKey {
    NameAsc,
    NameDesc,
    StatusEnabledFirst,
    SourceUserFirst,
    SourceSystemFirst,
}

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

fn build_ui(app: &Application) -> Result<()> {
    let entries = load_entries().unwrap_or_else(|err| {
        eprintln!("Failed to load entries: {err:?}");
        Vec::new()
    });

    let list_box = ListBox::new();
    list_box.set_accessible_role(AccessibleRole::List);
    list_box.set_selection_mode(SelectionMode::Single);

    let detail_name = Label::new(Some("-"));
    let detail_command = Label::new(Some("-"));
    let detail_source = Label::new(Some("-"));
    let detail_status = Label::new(Some("-"));
    let status_bar = Label::new(None);
    status_bar.set_wrap(true);

    let toggle_button = Button::with_label("Enable/Disable");
    let delete_button = Button::with_label("Delete");
    let edit_button = Button::with_label("Edit");
    let sort_button = Button::with_label("Sort");
    let about_button = Button::with_label("About");
    toggle_button.set_sensitive(false);
    delete_button.set_sensitive(false);
    edit_button.set_sensitive(false);

    let state = AppState {
        entries: Rc::new(RefCell::new(entries)),
        visible_indices: Rc::new(RefCell::new(Vec::new())),
        filter: Rc::new(RefCell::new(FilterState::default())),
        sort: Rc::new(Cell::new(SortKey::NameAsc)),
        selected: Rc::new(Cell::new(None)),
        list_box: list_box.clone(),
        detail_name,
        detail_command,
        detail_source,
        detail_status,
        status_bar: status_bar.clone(),
        toggle_button: toggle_button.clone(),
        delete_button: delete_button.clone(),
        edit_button: edit_button.clone(),
    };

    rebuild_list(&state);

    let refresh_button = Button::with_label("Refresh");
    refresh_button.set_accessible_role(AccessibleRole::Button);
    refresh_button.set_tooltip_text(Some("Refresh entries"));
    let add_button = Button::with_label("Add");
    add_button.set_accessible_role(AccessibleRole::Button);
    add_button.set_tooltip_text(Some("Add autostart entry"));
    let filter_button = Button::with_label("Filter");
    filter_button.set_accessible_role(AccessibleRole::Button);
    filter_button.set_tooltip_text(Some("Filter visible entries"));
    about_button.set_accessible_role(AccessibleRole::Button);
    about_button.set_tooltip_text(Some("About this app"));

    {
        let state = state.clone();
        refresh_button.connect_clicked(move |_| {
            if let Err(err) = refresh_entries(&state) {
                state.status_bar.set_text(&format!("Refresh failed: {err:#}"));
            }
        });
    }

    {
        let state = state.clone();
        add_button.connect_clicked(move |_| {
            if let Err(err) = show_add_dialog(&state) {
                state.status_bar.set_text(&format!("Add failed: {err:#}"));
            }
        });
    }

    {
        let state = state.clone();
        state.list_box.clone().connect_row_selected(move |_, row| {
            let idx = row
                .and_then(|r| usize::try_from(r.index()).ok())
                .and_then(|visible_idx| state.visible_indices.borrow().get(visible_idx).copied());
            state.selected.replace(idx);
            update_detail(&state);
        });
    }

    {
        let state = state.clone();
        filter_button.connect_clicked(move |_| {
            if let Err(err) = show_filter_dialog(&state) {
                state
                    .status_bar
                    .set_text(&format!("Filter dialog failed: {err:#}"));
            }
        });
    }

    {
        let state = state.clone();
        sort_button.connect_clicked(move |_| {
            if let Err(err) = show_sort_dialog(&state) {
                state
                    .status_bar
                    .set_text(&format!("Sort dialog failed: {err:#}"));
            }
        });
    }

    {
        let state = state.clone();
        about_button.connect_clicked(move |_| {
            if let Err(err) = show_about_dialog(&state) {
                state
                    .status_bar
                    .set_text(&format!("About dialog failed: {err:#}"));
            }
        });
    }

    {
        let state = state.clone();
        toggle_button.connect_clicked(move |_| {
            if let Err(err) = toggle_selected(&state) {
                state.status_bar.set_text(&format!("Toggle failed: {err:#}"));
            }
        });
    }

    {
        let state = state.clone();
        delete_button.connect_clicked(move |_| {
            if let Err(err) = delete_selected(&state) {
                state.status_bar.set_text(&format!("Delete failed: {err:#}"));
            }
        });
    }

    {
        let state = state.clone();
        edit_button.connect_clicked(move |_| {
            if let Err(err) = show_edit_dialog(&state) {
                state.status_bar.set_text(&format!("Edit failed: {err:#}"));
            }
        });
    }

    let header = HeaderBar::builder()
        .title_widget(&Label::new(Some("Universal Startup Manager")))
        .show_title_buttons(true)
        .build();
    header.pack_start(&refresh_button);
    header.pack_start(&filter_button);
    header.pack_start(&sort_button);
    header.pack_end(&add_button);
    header.pack_end(&about_button);

    let list_box_scrolled = ScrolledWindow::builder()
        .child(&list_box)
        .min_content_width(320)
        .build();

    let detail_box = GtkBox::new(Orientation::Vertical, 6);
    detail_box.append(&label_row("Name:", &state.detail_name));
    detail_box.append(&label_row("Command:", &state.detail_command));
    detail_box.append(&label_row("Source:", &state.detail_source));
    detail_box.append(&label_row("Status:", &state.detail_status));

    let action_row = GtkBox::new(Orientation::Horizontal, 6);
    toggle_button.set_accessible_role(AccessibleRole::Button);
    toggle_button.set_tooltip_text(Some("Toggle enabled state"));
    delete_button.set_accessible_role(AccessibleRole::Button);
    delete_button.set_tooltip_text(Some("Delete entry"));
    edit_button.set_accessible_role(AccessibleRole::Button);
    edit_button.set_tooltip_text(Some("Edit entry"));
    action_row.append(&toggle_button);
    action_row.append(&edit_button);
    action_row.append(&delete_button);
    detail_box.append(&action_row);
    detail_box.append(&Label::new(Some("Status messages:")));
    detail_box.append(&status_bar);

    let content = GtkBox::new(Orientation::Horizontal, 12);
    content.append(&list_box_scrolled);
    content.append(&detail_box);

    let root = GtkBox::new(Orientation::Vertical, 8);
    root.append(&header);
    root.append(&content);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Universal Startup Manager")
        .default_width(900)
        .default_height(600)
        .child(&root)
        .build();

    window.present();
    Ok(())
}

fn label_row(label: &str, value: &Label) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 6);
    let lab = Label::new(Some(label));
    lab.set_mnemonic_widget(Some(value));
    row.append(&lab);
    row.append(value);
    row
}

fn apply_filter(entries: &[StartupEntry], filter: &FilterState) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            let state_ok = (filter.show_enabled && entry.enabled)
                || (filter.show_disabled && !entry.enabled)
                || (!filter.show_enabled && !filter.show_disabled);
            let source_ok = (filter.show_user && matches!(entry.source, StartupSource::UserAutostart))
                || (filter.show_system && matches!(entry.source, StartupSource::SystemAutostart))
                || (!filter.show_user && !filter.show_system);
            state_ok && source_ok
        })
        .map(|(idx, _)| idx)
        .collect()
}

fn sort_indices(entries: &[StartupEntry], mut indices: Vec<usize>, sort: SortKey) -> Vec<usize> {
    indices.sort_by(|&a, &b| {
        let ea = &entries[a];
        let eb = &entries[b];
        match sort {
            SortKey::NameAsc => ea.name.to_lowercase().cmp(&eb.name.to_lowercase()),
            SortKey::NameDesc => eb.name.to_lowercase().cmp(&ea.name.to_lowercase()),
            SortKey::StatusEnabledFirst => {
                eb.enabled.cmp(&ea.enabled).then_with(|| ea.name.to_lowercase().cmp(&eb.name.to_lowercase()))
            }
            SortKey::SourceUserFirst => {
                let sa = matches!(ea.source, StartupSource::UserAutostart);
                let sb = matches!(eb.source, StartupSource::UserAutostart);
                sb.cmp(&sa).then_with(|| ea.name.to_lowercase().cmp(&eb.name.to_lowercase()))
            }
            SortKey::SourceSystemFirst => {
                let sa = matches!(ea.source, StartupSource::SystemAutostart);
                let sb = matches!(eb.source, StartupSource::SystemAutostart);
                sb.cmp(&sa).then_with(|| ea.name.to_lowercase().cmp(&eb.name.to_lowercase()))
            }
        }
    });
    indices
}

fn rebuild_list(state: &AppState) {
    while let Some(child) = state.list_box.first_child() {
        state.list_box.remove(&child);
    }
    let filtered = apply_filter(&state.entries.borrow(), &state.filter.borrow());
    let sorted = sort_indices(&state.entries.borrow(), filtered, state.sort.get());
    state.visible_indices.replace(sorted.clone());
    state.selected.replace(None);
    if sorted.is_empty() {
        let row = ListBoxRow::new();
        row.set_accessible_role(AccessibleRole::ListItem);
        row.set_child(Some(&Label::new(Some("No entries to show"))));
        state.list_box.append(&row);
        state.status_bar.set_text("No entries match the current filter");
        return;
    }
    for idx in sorted {
        let entry = &state.entries.borrow()[idx];
        let text = format!(
            "{} — {} [{}] {}",
            entry.name,
            entry.command,
            source_label(&entry.source),
            if entry.enabled { "enabled" } else { "disabled" }
        );
        let row = ListBoxRow::new();
        row.set_accessible_role(AccessibleRole::ListItem);
        row.set_child(Some(&Label::new(Some(&text))));
        state.list_box.append(&row);
    }
}

fn refresh_entries(state: &AppState) -> Result<()> {
    let new_entries = load_entries()?;
    state.entries.replace(new_entries);
    state.selected.replace(None);
    rebuild_list(state);
    update_detail(state);
    state.status_bar.set_text("Refreshed");
    Ok(())
}

fn update_detail(state: &AppState) {
    if let Some(idx) = state.selected.get() {
        if let Some(entry) = state.entries.borrow().get(idx) {
            state.detail_name.set_text(&entry.name);
            state.detail_command.set_text(&entry.command);
            state.detail_source.set_text(source_label(&entry.source));
            state
                .detail_status
                .set_text(if entry.enabled { "enabled" } else { "disabled" });
            let user_owned = matches!(entry.source, StartupSource::UserAutostart)
                && entry
                    .path
                    .as_ref()
                    .map(|p| is_user_owned_path(p))
                    .unwrap_or(false);
            state.toggle_button.set_sensitive(user_owned);
            state.delete_button.set_sensitive(user_owned);
            state.edit_button.set_sensitive(user_owned);
            return;
        }
    }
    state.detail_name.set_text("-");
    state.detail_command.set_text("-");
    state.detail_source.set_text("-");
    state.detail_status.set_text("-");
    state.toggle_button.set_sensitive(false);
    state.delete_button.set_sensitive(false);
    state.edit_button.set_sensitive(false);
}

fn toggle_selected(state: &AppState) -> Result<()> {
    let idx = state.selected.get().context("No item selected")?;
    let mut entries = state.entries.borrow_mut();
    let entry = entries.get_mut(idx).context("Invalid selection")?;
    if entry.source != StartupSource::UserAutostart {
        bail!("Only user autostart entries can be toggled");
    }
    let path = entry
        .path
        .clone()
        .unwrap_or_else(|| user_autostart_dir().join(format!("{}.desktop", slugify(&entry.name))));
    let path = validate_user_entry_path(&path)?;
    entry.enabled = !entry.enabled;
    write_desktop_entry(entry, &path)?;
    state
        .status_bar
        .set_text(if entry.enabled { "Enabled" } else { "Disabled" });
    refresh_entries(state)?;
    Ok(())
}

fn delete_selected(state: &AppState) -> Result<()> {
    let idx = state.selected.get().context("No item selected")?;
    let entries = state.entries.borrow();
    let entry = entries.get(idx).context("Invalid selection")?;
    if entry.source != StartupSource::UserAutostart {
        bail!("Only user autostart entries can be deleted");
    }
    let path = entry
        .path
        .as_ref()
        .context("Entry has no associated file path")?;
    let path = validate_user_entry_path(path)?;
    fs::remove_file(&path).with_context(|| format!("Removing {:?}", path))?;
    drop(entries);
    state.status_bar.set_text("Deleted entry");
    refresh_entries(state)?;
    Ok(())
}

fn show_add_dialog(state: &AppState) -> Result<()> {
    let parent = state
        .list_box
        .root()
        .and_then(|w| w.downcast::<ApplicationWindow>().ok());
    let dialog = Dialog::with_buttons(
        Some("Add autostart entry"),
        parent.as_ref(),
        gtk4::DialogFlags::MODAL,
        &[("Cancel", ResponseType::Cancel), ("Add", ResponseType::Ok)],
    );

    let content = dialog.content_area();
    content.set_spacing(6);
    let name_label = Label::new(Some("Name:"));
    let name_entry = Entry::new();
    name_entry.set_placeholder_text(Some("Name"));
    name_entry.set_accessible_role(AccessibleRole::TextBox);
    name_label.set_mnemonic_widget(Some(&name_entry));

    let cmd_label = Label::new(Some("Command:"));
    let cmd_entry = Entry::new();
    cmd_entry.set_placeholder_text(Some("Command"));
    cmd_entry.set_accessible_role(AccessibleRole::TextBox);
    cmd_label.set_mnemonic_widget(Some(&cmd_entry));

    content.append(&name_label);
    content.append(&name_entry);
    content.append(&cmd_label);
    content.append(&cmd_entry);

    dialog.connect_response({
        let state = state.clone();
        move |dlg, resp| {
            if resp == ResponseType::Ok {
                let name = name_entry.text().to_string();
                let cmd = cmd_entry.text().to_string();
                if let Err(err) = create_user_entry(&name, &cmd) {
                    state
                        .status_bar
                        .set_text(&format!("Failed to add entry: {err:#}"));
                } else if let Err(err) = refresh_entries(&state) {
                    state
                        .status_bar
                        .set_text(&format!("Failed to refresh after add: {err:#}"));
                } else {
                    state.status_bar.set_text("Added entry");
                }
            }
            dlg.close();
        }
    });

    dialog.show();
    Ok(())
}

fn show_edit_dialog(state: &AppState) -> Result<()> {
    let idx = state.selected.get().context("No item selected")?;
    let entry = {
        let entries = state.entries.borrow();
        entries.get(idx).cloned().context("Invalid selection")?
    };
    if entry.source != StartupSource::UserAutostart {
        bail!("Only user entries can be edited");
    }

    let parent = state
        .list_box
        .root()
        .and_then(|w| w.downcast::<ApplicationWindow>().ok());
    let dialog = Dialog::with_buttons(
        Some("Edit autostart entry"),
        parent.as_ref(),
        gtk4::DialogFlags::MODAL,
        &[("Cancel", ResponseType::Cancel), ("Save", ResponseType::Ok)],
    );

    let content = dialog.content_area();
    content.set_spacing(6);
    let name_label = Label::new(Some("Name:"));
    let name_entry = Entry::new();
    name_entry.set_placeholder_text(Some("Name"));
    name_entry.set_text(&entry.name);
    name_entry.set_accessible_role(AccessibleRole::TextBox);
    name_label.set_mnemonic_widget(Some(&name_entry));

    let cmd_label = Label::new(Some("Command:"));
    let cmd_entry = Entry::new();
    cmd_entry.set_placeholder_text(Some("Command"));
    cmd_entry.set_text(&entry.command);
    cmd_entry.set_accessible_role(AccessibleRole::TextBox);
    cmd_label.set_mnemonic_widget(Some(&cmd_entry));

    content.append(&name_label);
    content.append(&name_entry);
    content.append(&cmd_label);
    content.append(&cmd_entry);

    dialog.connect_response({
        let state = state.clone();
        let original_path = entry.path.clone();
        move |dlg, resp| {
            if resp == ResponseType::Ok {
                let new_name = name_entry.text().to_string();
                let new_cmd = cmd_entry.text().to_string();
                if new_name.trim().is_empty() || new_cmd.trim().is_empty() {
                    state
                        .status_bar
                        .set_text("Name and command cannot be empty");
                    dlg.close();
                    return;
                }
                let res = edit_user_entry(&entry, &new_name, &new_cmd, original_path.as_ref());
                if let Err(err) = res {
                    state
                        .status_bar
                        .set_text(&format!("Failed to save: {err:#}"));
                } else if let Err(err) = refresh_entries(&state) {
                    state
                        .status_bar
                        .set_text(&format!("Failed to refresh after edit: {err:#}"));
                } else {
                    state.status_bar.set_text("Saved entry");
                }
            }
            dlg.close();
        }
    });

    dialog.show();
    Ok(())
}

fn show_filter_dialog(state: &AppState) -> Result<()> {
    let parent = state
        .list_box
        .root()
        .and_then(|w| w.downcast::<ApplicationWindow>().ok());
    let dialog = Dialog::with_buttons(
        Some("Filter entries"),
        parent.as_ref(),
        gtk4::DialogFlags::MODAL,
        &[("Cancel", ResponseType::Cancel), ("Apply", ResponseType::Ok)],
    );

    let content = dialog.content_area();
    content.set_spacing(8);
    let current = *state.filter.borrow();
    let enabled_cb = CheckButton::with_label("Show enabled");
    enabled_cb.set_active(current.show_enabled);
    let disabled_cb = CheckButton::with_label("Show disabled");
    disabled_cb.set_active(current.show_disabled);
    let user_cb = CheckButton::with_label("Show user entries");
    user_cb.set_active(current.show_user);
    let system_cb = CheckButton::with_label("Show system entries");
    system_cb.set_active(current.show_system);

    content.append(&enabled_cb);
    content.append(&disabled_cb);
    content.append(&user_cb);
    content.append(&system_cb);

    dialog.connect_response({
        let state = state.clone();
        move |dlg, resp| {
            if resp == ResponseType::Ok {
                let mut filter = state.filter.borrow_mut();
                filter.show_enabled = enabled_cb.is_active();
                filter.show_disabled = disabled_cb.is_active();
                filter.show_user = user_cb.is_active();
                filter.show_system = system_cb.is_active();
                drop(filter);
                rebuild_list(&state);
                update_detail(&state);
                state.status_bar.set_text("Filter applied");
            }
            dlg.close();
        }
    });

    dialog.show();
    Ok(())
}

fn show_sort_dialog(state: &AppState) -> Result<()> {
    let parent = state
        .list_box
        .root()
        .and_then(|w| w.downcast::<ApplicationWindow>().ok());
    let dialog = Dialog::with_buttons(
        Some("Sort entries"),
        parent.as_ref(),
        gtk4::DialogFlags::MODAL,
        &[("Cancel", ResponseType::Cancel), ("Apply", ResponseType::Ok)],
    );

    let content = dialog.content_area();
    content.set_spacing(8);
    let current = state.sort.get();

    let name_asc = CheckButton::with_label("Name (A→Z)");
    name_asc.set_group(None::<&CheckButton>);
    name_asc.set_active(matches!(current, SortKey::NameAsc));

    let name_desc = CheckButton::with_label("Name (Z→A)");
    name_desc.set_group(Some(&name_asc));
    name_desc.set_active(matches!(current, SortKey::NameDesc));

    let status = CheckButton::with_label("Status (enabled first)");
    status.set_group(Some(&name_asc));
    status.set_active(matches!(current, SortKey::StatusEnabledFirst));

    let source_user = CheckButton::with_label("Source (user first)");
    source_user.set_group(Some(&name_asc));
    source_user.set_active(matches!(current, SortKey::SourceUserFirst));

    let source_system = CheckButton::with_label("Source (system first)");
    source_system.set_group(Some(&name_asc));
    source_system.set_active(matches!(current, SortKey::SourceSystemFirst));

    content.append(&name_asc);
    content.append(&name_desc);
    content.append(&status);
    content.append(&source_user);
    content.append(&source_system);

    dialog.connect_response({
        let state = state.clone();
        move |dlg, resp| {
            if resp == ResponseType::Ok {
                let new_sort = if name_asc.is_active() {
                    SortKey::NameAsc
                } else if name_desc.is_active() {
                    SortKey::NameDesc
                } else if status.is_active() {
                    SortKey::StatusEnabledFirst
                } else if source_user.is_active() {
                    SortKey::SourceUserFirst
                } else if source_system.is_active() {
                    SortKey::SourceSystemFirst
                } else {
                    state.sort.get()
                };
                state.sort.set(new_sort);
                rebuild_list(&state);
                state.status_bar.set_text("Sort applied");
            }
            dlg.close();
        }
    });

    dialog.show();
    Ok(())
}

fn show_about_dialog(state: &AppState) -> Result<()> {
    let parent = state
        .list_box
        .root()
        .and_then(|w| w.downcast::<ApplicationWindow>().ok());
    let dialog = Dialog::with_buttons(
        Some("About Universal Startup Manager"),
        parent.as_ref(),
        gtk4::DialogFlags::MODAL,
        &[("Close", ResponseType::Close)],
    );
    dialog.set_accessible_role(AccessibleRole::Dialog);
    dialog.update_property(&[gtk4::accessible::Property::Label(
        "About Universal Startup Manager",
    )]);

    let content = dialog.content_area();
    content.set_spacing(6);
    let description = Label::new(Some(&format!(
        "Manage user autostart entries and view system startup items. Version {}",
        env!("CARGO_PKG_VERSION")
    )));
    description.set_wrap(true);
    content.append(&description);

    let close_button = dialog
        .widget_for_response(ResponseType::Close)
        .and_then(|w| w.downcast::<Button>().ok());
    if let Some(close_button) = close_button {
        close_button.update_property(&[gtk4::accessible::Property::Label(
            "Close about dialog",
        )]);
    }
    dialog.connect_response(|dlg, _| {
        dlg.close();
    });
    dialog.present();
    Ok(())
}

fn load_entries() -> Result<Vec<StartupEntry>> {
    let mut entries = Vec::new();
    entries.extend(load_autostart_dir(
        user_autostart_dir().as_ref(),
        StartupSource::UserAutostart,
    )?);
    entries.extend(load_autostart_dir(
        system_autostart_dir().as_ref(),
        StartupSource::SystemAutostart,
    )?);
    Ok(entries)
}

fn user_autostart_dir() -> PathBuf {
    let mut base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
    base.push("autostart");
    base
}

fn system_autostart_dir() -> PathBuf {
    PathBuf::from("/etc/xdg/autostart")
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

fn parse_desktop_file(path: &Path, source: StartupSource) -> Result<StartupEntry> {
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
    let mut current_other: Vec<String> = Vec::new();

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // close previous non-entry group buffer
            if let Some(group) = current_group.take() {
                if group != "Desktop Entry" && !current_other.is_empty() {
                    other_groups.push(current_other.clone());
                } else if group == "Desktop Entry" {
                    // drop, we rebuild entry
                }
                current_other.clear();
            } else {
                // preamble ends here
                if !current_other.is_empty() {
                    preamble.append(&mut current_other);
                }
            }

            let group_name = trimmed.trim_matches(&['[', ']'][..]).to_string();
            let in_entry_group = group_name == "Desktop Entry";
            current_group = Some(group_name.clone());
            if !in_entry_group {
                current_other.push(raw_line.to_string());
            }
            continue;
        }

        if let Some(group) = &current_group {
            if group == "Desktop Entry" {
                if trimmed.starts_with('#') || trimmed.is_empty() {
                    entry_comments.push(raw_line.to_string());
                    continue;
                }
                let (key, value) = match raw_line.split_once('=') {
                    Some(pair) => pair,
                    None => continue,
                };
                let key = key.trim();
                let value = value.trim();
                if key == "Name" {
                    name = value.to_string();
                } else if let Some(locale) = key.strip_prefix("Name[") {
                    if let Some(locale_key) = locale.strip_suffix(']') {
                        localized_names.push((locale_key.to_string(), value.to_string()));
                    }
                } else if key == "Exec" {
                    command = value.to_string();
                } else if key == "Hidden" {
                    enabled = value != "true";
                } else if key == "X-GNOME-Autostart-enabled" {
                    enabled = value == "true";
                } else {
                    extra.push((key.to_string(), value.to_string()));
                }
            } else {
                current_other.push(raw_line.to_string());
            }
        } else {
            preamble.push(raw_line.to_string());
        }
    }

    // Flush last group buffer if it is non-entry.
    if let Some(group) = current_group {
        if group != "Desktop Entry" && !current_other.is_empty() {
            other_groups.push(current_other);
        } else if group == "Desktop Entry" {
            // drop, already parsed into fields
        }
    } else if !current_other.is_empty() {
        preamble.extend(current_other);
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

fn write_desktop_entry(entry: &StartupEntry, path: &Path) -> Result<()> {
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

fn edit_user_entry(original: &StartupEntry, new_name: &str, new_cmd: &str, original_path: Option<&PathBuf>) -> Result<()> {
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

fn create_user_entry(name: &str, command: &str) -> Result<PathBuf> {
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

fn slugify(name: &str) -> String {
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

fn source_label(source: &StartupSource) -> &'static str {
    match source {
        StartupSource::UserAutostart => "user",
        StartupSource::SystemAutostart => "system",
        StartupSource::ShellProfile => "shell",
        StartupSource::Unknown => "unknown",
    }
}

fn is_user_owned_path(path: &Path) -> bool {
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

fn validate_user_entry_path(path: &Path) -> Result<PathBuf> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::read_to_string;

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
        let filter = FilterState { show_enabled: true, show_disabled: false, show_user: true, show_system: true };
        let filtered = apply_filter(&entries, &filter);
        assert_eq!(filtered.len(), 2);
        let sorted = sort_indices(&entries, filtered, SortKey::NameAsc);
        let names: Vec<_> = sorted.iter().map(|i| entries[*i].name.as_str()).collect();
        assert_eq!(names, vec!["B", "C"]);
        let sorted_status = sort_indices(&entries, vec![0,1,2], SortKey::StatusEnabledFirst);
        assert_eq!(sorted_status[0], 0); // enabled first
    }

    #[test]
    fn filter_combined_user_enabled() {
        let entries = vec![
            entry("UserEnabled", "/bin/true", true, StartupSource::UserAutostart),
            entry("UserDisabled", "/bin/true", false, StartupSource::UserAutostart),
            entry("SystemEnabled", "/bin/true", true, StartupSource::SystemAutostart),
        ];
        let filter = FilterState { show_enabled: true, show_disabled: false, show_user: true, show_system: false };
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
        assert_eq!(entry.extra.iter().find(|(k, _)| k == "X-Test").map(|(_, v)| v.as_str()), Some("1"));
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
}
