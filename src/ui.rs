use std::cell::{Cell, RefCell};
use std::fs;
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use gtk4::prelude::*;
use gtk4::{
    AccessibleRole, Application, ApplicationWindow, Box as GtkBox, Button, CheckButton, Dialog,
    Entry, HeaderBar, Label, ListBox, ListBoxRow, Orientation, ResponseType, ScrolledWindow,
    SelectionMode,
};

use crate::autostart::{
    create_user_entry, edit_user_entry, is_user_owned_path, load_entries, user_autostart_dir,
    validate_user_entry_path,
};
use crate::desktop::{slugify, source_label, write_desktop_entry};
use crate::types::{FilterState, SortKey, StartupEntry, StartupSource};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) entries: Rc<RefCell<Vec<StartupEntry>>>,
    pub(crate) visible_indices: Rc<RefCell<Vec<usize>>>,
    pub(crate) filter: Rc<RefCell<FilterState>>,
    pub(crate) sort: Rc<Cell<SortKey>>,
    pub(crate) selected: Rc<Cell<Option<usize>>>,
    pub(crate) list_box: ListBox,
    pub(crate) detail_name: Label,
    pub(crate) detail_command: Label,
    pub(crate) detail_source: Label,
    pub(crate) detail_status: Label,
    pub(crate) status_bar: Label,
    pub(crate) toggle_button: Button,
    pub(crate) delete_button: Button,
    pub(crate) edit_button: Button,
}

pub(crate) fn build_ui(app: &Application) -> Result<()> {
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

pub(crate) fn apply_filter(entries: &[StartupEntry], filter: &FilterState) -> Vec<usize> {
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

pub(crate) fn sort_indices(entries: &[StartupEntry], mut indices: Vec<usize>, sort: SortKey) -> Vec<usize> {
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

    dialog.present();
    Ok(())
}

fn show_edit_dialog(state: &AppState) -> Result<()> {
    let idx = state.selected.get().context("No item selected")?;
    let entries = state.entries.borrow();
    let entry = entries.get(idx).context("Invalid selection")?;
    if entry.source != StartupSource::UserAutostart {
        bail!("Only user autostart entries can be edited");
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
    name_entry.set_text(&entry.name);
    name_entry.set_accessible_role(AccessibleRole::TextBox);
    name_label.set_mnemonic_widget(Some(&name_entry));

    let cmd_label = Label::new(Some("Command:"));
    let cmd_entry = Entry::new();
    cmd_entry.set_text(&entry.command);
    cmd_entry.set_accessible_role(AccessibleRole::TextBox);
    cmd_label.set_mnemonic_widget(Some(&cmd_entry));

    content.append(&name_label);
    content.append(&name_entry);
    content.append(&cmd_label);
    content.append(&cmd_entry);

    dialog.connect_response({
        let state = state.clone();
        let entry = entry.clone();
        let original_path = entry.path.clone();
        move |dlg, resp| {
            if resp == ResponseType::Ok {
                let name = name_entry.text().to_string();
                let cmd = cmd_entry.text().to_string();
                if let Err(err) = edit_user_entry(&entry, &name, &cmd, original_path.as_ref()) {
                    state
                        .status_bar
                        .set_text(&format!("Failed to edit: {err:#}"));
                } else if let Err(err) = refresh_entries(&state) {
                    state
                        .status_bar
                        .set_text(&format!("Failed to refresh after edit: {err:#}"));
                } else {
                    state.status_bar.set_text("Saved changes");
                }
            }
            dlg.close();
        }
    });

    dialog.present();
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
    content.set_spacing(6);
    let current = state.filter.borrow().clone();
    let enabled_cb = CheckButton::with_label("Show enabled entries");
    enabled_cb.set_active(current.show_enabled);
    let disabled_cb = CheckButton::with_label("Show disabled entries");
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
                rebuild_list(&state);
            }
            dlg.close();
        }
    });

    dialog.present();
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
    content.set_spacing(6);
    let current = state.sort.get();

    let name_asc = CheckButton::with_label("Name (A to Z)");
    let name_desc = CheckButton::with_label("Name (Z to A)");
    let status_first = CheckButton::with_label("Status (enabled first)");
    let source_user = CheckButton::with_label("Source (user first)");
    let source_system = CheckButton::with_label("Source (system first)");
    name_desc.set_group(Some(&name_asc));
    status_first.set_group(Some(&name_asc));
    source_user.set_group(Some(&name_asc));
    source_system.set_group(Some(&name_asc));
    match current {
        SortKey::NameAsc => name_asc.set_active(true),
        SortKey::NameDesc => name_desc.set_active(true),
        SortKey::StatusEnabledFirst => status_first.set_active(true),
        SortKey::SourceUserFirst => source_user.set_active(true),
        SortKey::SourceSystemFirst => source_system.set_active(true),
    }
    content.append(&name_asc);
    content.append(&name_desc);
    content.append(&status_first);
    content.append(&source_user);
    content.append(&source_system);

    dialog.connect_response({
        let state = state.clone();
        move |dlg, resp| {
            if resp == ResponseType::Ok {
                let next_sort = if name_desc.is_active() {
                    SortKey::NameDesc
                } else if status_first.is_active() {
                    SortKey::StatusEnabledFirst
                } else if source_user.is_active() {
                    SortKey::SourceUserFirst
                } else if source_system.is_active() {
                    SortKey::SourceSystemFirst
                } else {
                    SortKey::NameAsc
                };
                state.sort.set(next_sort);
                rebuild_list(&state);
            }
            dlg.close();
        }
    });

    dialog.present();
    Ok(())
}

fn show_about_dialog(_state: &AppState) -> Result<()> {
    let dialog = Dialog::with_buttons(
        Some("About Universal Startup Manager"),
        None::<&ApplicationWindow>,
        gtk4::DialogFlags::MODAL,
        &[("Close", ResponseType::Close)],
    );

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
