//! Shared data types, constants and serde defaults.

use crate::*;

// Must match the frontend AliasEntry shape. serde's camelCase conversion keeps
// Rust idiomatic while still producing JSON fields like customCommand/createdAt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AliasEntry {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) action: String,
    pub(crate) custom_command: Option<String>,
    pub(crate) command_preview: String,
    #[serde(default)]
    pub(crate) favorite: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

// Simple legacy command files discovered in user-owned PATH directories.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommandFileCandidate {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) source_file: String,
    #[serde(skip)]
    pub(crate) source_path: PathBuf,
}

// State returned to the frontend on load/save. Besides aliases, it contains
// display paths and setup status for the UI header.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppState {
    pub(crate) aliases: Vec<AliasEntry>,
    pub(crate) config_file: String,
    // Directory containing generated commands such as test1.cmd.
    pub(crate) command_dir: String,
    // Absolute command_dir value, shown when the user needs to restart Terminal.
    pub(crate) path_entry: String,
    // True when command_dir is already visible through User PATH or process PATH.
    pub(crate) path_configured: bool,
    pub(crate) import_candidates: Vec<CommandFileCandidate>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportResult {
    pub(crate) state: AppState,
    pub(crate) imported_count: usize,
    pub(crate) backup_dir: String,
    pub(crate) warning: Option<String>,
}

// Portable backups are deliberately wrapped in a versioned envelope instead
// of exposing config.json directly. This gives future releases room to evolve
// the format while rejecting unrelated or malformed JSON files today.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AliasBackup {
    pub(crate) format: String,
    pub(crate) version: u32,
    pub(crate) exported_at: String,
    pub(crate) aliases: Vec<AliasEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupExportResult {
    pub(crate) file: String,
    pub(crate) exported_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupImportResult {
    pub(crate) state: AppState,
    pub(crate) imported_count: usize,
    pub(crate) replaced_count: usize,
}

// Deleted aliases live in a separate file so config.json remains fully
// backwards-compatible. deleted_at is Unix time, which makes the 30-day
// retention rule independent from locale and frontend date parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrashEntry {
    pub(crate) alias: AliasEntry,
    pub(crate) deleted_at: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrashMutationResult {
    pub(crate) state: AppState,
    pub(crate) trash: Vec<TrashEntry>,
}

// Automations are intentionally stored separately from aliases. Each workflow
// has one working directory and an ordered list of commands or timed pauses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationStep {
    pub(crate) id: String,
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) seconds: u64,
    #[serde(default = "default_command_behavior")]
    pub(crate) behavior: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Automation {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) steps: Vec<AutomationStep>,
    #[serde(default)]
    pub(crate) favorite: bool,
    // A free-text label the UI groups and filters automations by. Empty
    // means "ungrouped"; older automations.json files without this field
    // default to that.
    #[serde(default)]
    pub(crate) group: String,
    // Optional global keyboard shortcut (Tauri accelerator string, e.g.
    // "CmdOrCtrl+Shift+L") that runs this automation from anywhere while
    // EasyAlias is running. Older automations.json files default to None.
    #[serde(default)]
    pub(crate) hotkey: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

// A timed automation schedules an existing Automation to run at a wall-clock
// time, or at that day's actual sunrise/sunset, optionally on specific
// weekdays, through the OS's own scheduler (Task Scheduler on Windows) so it
// fires even while EasyAlias is not running. `days` uses lowercase
// three-letter abbreviations ("mon".."sun"); empty means every day. Run
// history is intentionally just the most recent attempt - there is no UI
// for a full log, only "did the last run work".
//
// Clock-time entries each get their own exact-fire Scheduled Task
// (unchanged from before). Sunrise/sunset entries instead ride along on one
// shared periodic "sun checker" task, since their fire time shifts by
// roughly a minute a day and so can't be baked into a fixed daily trigger;
// `last_triggered_date` stops that periodic check from firing an entry more
// than once on the same calendar day.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TimedAutomation {
    pub(crate) id: String,
    pub(crate) automation_id: String,
    #[serde(default = "default_trigger_kind")]
    pub(crate) trigger_kind: String,
    #[serde(default)]
    pub(crate) time: String,
    #[serde(default)]
    pub(crate) days: Vec<String>,
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    #[serde(default)]
    pub(crate) last_run_at: Option<u64>,
    #[serde(default)]
    pub(crate) last_run_status: Option<String>,
    #[serde(default)]
    pub(crate) last_run_output: Option<String>,
    #[serde(default)]
    pub(crate) last_triggered_date: Option<String>,
}

pub(crate) fn default_trigger_kind() -> String {
    "clock".to_string()
}

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

// Sunrise/sunset triggers only need an approximate location - close enough
// that "sunrise" fires within a few minutes of the real one - not a precise
// street address, so this is a small fixed table of region keys mapped to a
// representative city's coordinates, picked from a dropdown instead of
// typing exact latitude/longitude.
pub(crate) const SUN_REGIONS: &[(&str, &str, f64, f64)] = &[
    ("us-pacific", "US Pacific (Los Angeles)", 34.05, -118.24),
    ("us-mountain", "US Mountain (Denver)", 39.74, -104.99),
    ("us-central", "US Central (Chicago)", 41.88, -87.63),
    ("us-eastern", "US Eastern (New York)", 40.71, -74.01),
    ("uk-ireland", "UK & Ireland (London)", 51.51, -0.13),
    ("eu-west", "EU West (Paris)", 48.86, 2.35),
    ("eu-central", "EU Central (Berlin)", 52.52, 13.40),
    ("eu-east", "EU East (Kyiv)", 50.45, 30.52),
    ("asia-east", "East Asia (Tokyo)", 35.68, 139.69),
    ("asia-south", "South Asia (Delhi)", 28.61, 77.21),
    ("australia", "Australia (Sydney)", -33.87, 151.21),
];

pub(crate) fn sun_region_coordinates(region: &str) -> Option<(f64, f64)> {
    SUN_REGIONS
        .iter()
        .find(|(key, _, _, _)| *key == region)
        .map(|(_, _, latitude, longitude)| (*latitude, *longitude))
}

// A single global setting (not per-automation - the user has one physical
// location) that all sunrise/sunset timed automations share.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SunLocationSetting {
    pub(crate) region: String,
}

pub(crate) fn default_sun_location() -> SunLocationSetting {
    SunLocationSetting {
        region: "eu-central".to_string(),
    }
}

// Preferences of the terminal app, stored in ~/.easyalias/tui-settings.json.
// They are kept apart from the desktop app's settings.json on purpose: the
// theme of a terminal UI and of a window are separate choices. Every field has
// a default so an older or partial file still loads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TuiSettings {
    // "system" keeps the terminal's own colors; "light"/"dark" force a palette.
    #[serde(default = "default_theme")]
    pub(crate) theme: String,
    // Whether the alias view offers its built-in alias suggestions.
    #[serde(default = "default_true")]
    pub(crate) show_suggestions: bool,
}

pub(crate) fn default_theme() -> String {
    "system".to_string()
}

pub(crate) fn default_tui_settings() -> TuiSettings {
    TuiSettings {
        theme: default_theme(),
        show_suggestions: true,
    }
}

pub(crate) const THEME_VALUES: [&str; 3] = ["system", "light", "dark"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationTrashEntry {
    pub(crate) automation: Automation,
    pub(crate) deleted_at: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationTrashMutationResult {
    pub(crate) automations: Vec<Automation>,
    pub(crate) trash: Vec<AutomationTrashEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationBackup {
    pub(crate) format: String,
    pub(crate) version: u32,
    pub(crate) exported_at: String,
    pub(crate) automations: Vec<Automation>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationBackupImportResult {
    pub(crate) automations: Vec<Automation>,
    pub(crate) imported_count: usize,
    pub(crate) replaced_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationCommandResult {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) process_id: Option<u32>,
}

// One automation run gets one persistent cmd.exe process, so `cd` and `set`
// environment variables carry over between steps exactly as they would in a
// real Command Prompt window. A background thread streams its merged
// stdout/stderr line by line into `output_rx`; commands are matched to their
// output by writing a unique sentinel after each one.
pub(crate) struct AutomationSessionHandle {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) output_rx: mpsc::Receiver<String>,
}


pub(crate) const AUTOMATION_DONE_MARKER: &str = "__EASYALIAS_AUTOMATION_DONE__";
pub(crate) const AUTOMATION_BG_MARKER: &str = "__EASYALIAS_AUTOMATION_BG__";

// Reserved by the EasyAlias desktop app (`easya` opens it), so the TUI never
// creates an alias with that name either.
pub(crate) const APP_ALIAS_NAME: &str = "easya";
pub(crate) const IMPORT_MARKER_CONTENT: &str = "legacy command import prompt handled\n";
pub(crate) const BACKUP_FORMAT: &str = "easyalias-backup";
pub(crate) const BACKUP_VERSION: u32 = 1;
pub(crate) const AUTOMATION_BACKUP_FORMAT: &str = "easyalias-automation-backup";
pub(crate) const AUTOMATION_BACKUP_VERSION: u32 = 1;
pub(crate) const MAX_BACKUP_BYTES: u64 = 5 * 1024 * 1024;
pub(crate) const MAX_BACKUP_ALIASES: usize = 5000;
pub(crate) const TRASH_RETENTION_SECONDS: u64 = 30 * 24 * 60 * 60;
pub(crate) const MAX_AUTOMATIONS: usize = 200;
pub(crate) const MAX_AUTOMATION_STEPS: usize = 100;
pub(crate) const MAX_AUTOMATION_COMMAND_BYTES: usize = 16 * 1024;
pub(crate) const MAX_AUTOMATION_OUTPUT_CHARS: usize = 20_000;
pub(crate) const MAX_WAIT_SECONDS: u64 = 24 * 60 * 60;
// How often the shared sunrise/sunset checker task re-evaluates every
// sun-triggered timed automation. Tighter than a cosmetic theme check would
// need, since this actually drives real automation runs.
pub(crate) const SUN_CHECK_INTERVAL_MINUTES: u32 = 5;

pub(crate) fn default_command_behavior() -> String {
    "wait".to_string()
}
