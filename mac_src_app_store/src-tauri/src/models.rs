//! Shared data types, constants and serde defaults.

use crate::*;

// Must match the frontend AliasEntry shape. serde keeps Rust field names
// idiomatic while exposing camelCase JSON to TypeScript.
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

// Only conservative, single-line aliases are offered for migration. The
// source file keeps imports verifiable when zsh and Bash files are scanned.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ShellAliasCandidate {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) line_number: usize,
    pub(crate) source_file: String,
}

// State sent to the WebView. The Store edition exposes connection state rather
// than assuming unrestricted access to the user's home directory.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppState {
    pub(crate) aliases: Vec<AliasEntry>,
    pub(crate) config_file: String,
    pub(crate) alias_target: String,
    pub(crate) home_path: Option<String>,
    pub(crate) home_connected: bool,
    pub(crate) shell_config_file: String,
    pub(crate) managed_block_present: bool,
    pub(crate) connection_error: Option<String>,
    pub(crate) import_candidates: Vec<ShellAliasCandidate>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportResult {
    pub(crate) state: AppState,
    pub(crate) imported_count: usize,
    pub(crate) backup_file: String,
}

// Portable backups use a versioned envelope rather than exposing the internal
// container config directly. This keeps restores compatible across editions.
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

pub(crate) const MANAGED_BLOCK_START: &str = "# >>> EasyAlias managed aliases >>>";
pub(crate) const MANAGED_BLOCK_END: &str = "# <<< EasyAlias managed aliases <<<";
pub(crate) const IMPORT_MARKER_CONTENT: &str = "shell alias import prompt handled\n";
pub(crate) const RESERVED_ALIAS_NAME: &str = "easya";
pub(crate) const SHELL_STARTUP_FILES: [&str; 3] = [".zshrc", ".bash_profile", ".bashrc"];
pub(crate) const BACKUP_FORMAT: &str = "easyalias-backup";
pub(crate) const BACKUP_VERSION: u32 = 1;
pub(crate) const MAX_BACKUP_BYTES: u64 = 5 * 1024 * 1024;
pub(crate) const MAX_BACKUP_ALIASES: usize = 5000;
pub(crate) const TRASH_RETENTION_SECONDS: u64 = 30 * 24 * 60 * 60;
