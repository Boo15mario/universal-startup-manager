use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum StartupSource {
    UserAutostart,
    SystemAutostart,
    ShellProfile,
    Unknown,
}

#[derive(Debug, Clone)]
pub(crate) struct StartupEntry {
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) enabled: bool,
    pub(crate) source: StartupSource,
    pub(crate) path: Option<PathBuf>,
    pub(crate) extra: Vec<(String, String)>, // preserve additional keys in Desktop Entry group
    pub(crate) localized_names: Vec<(String, String)>, // locale -> name
    pub(crate) entry_comments: Vec<String>,            // comments/blank lines inside Desktop Entry
    pub(crate) preamble: Vec<String>,                  // lines before first group
    pub(crate) other_groups: Vec<Vec<String>>,         // raw lines for non-Desktop Entry groups
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FilterState {
    pub(crate) show_enabled: bool,
    pub(crate) show_disabled: bool,
    pub(crate) show_user: bool,
    pub(crate) show_system: bool,
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
pub(crate) enum SortKey {
    NameAsc,
    NameDesc,
    StatusEnabledFirst,
    SourceUserFirst,
    SourceSystemFirst,
}
