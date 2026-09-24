//! Alias command generation and validation (port of the desktop frontend's
//! aliases/command.ts), plus id/timestamp helpers.

use crate::*;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const ACTIONS: [&str; 6] = [
    "navigate",
    "open",
    "execute",
    "compile_gradle",
    "compile_maven",
    "custom",
];

pub(crate) fn action_label(action: &str) -> &'static str {
    match action {
        "navigate" => "Go to Folder",
        "open" => "Open",
        "execute" => "Run",
        "compile_gradle" => "Gradle Build",
        "compile_maven" => "Maven Build",
        "custom" => "Custom Command",
        _ => "Unknown",
    }
}

// Escape characters that can break a double-quoted zsh string.
pub(crate) fn escape_double_quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
}

// Converts a user-entered path into a safe Bash/zsh command argument.
// "~/" is expanded to "$HOME/" so generated aliases keep working reliably.
pub(crate) fn shell_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed == "~" {
        return "\"$HOME\"".to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return format!("\"$HOME/{}\"", escape_double_quoted(rest));
    }
    format!("\"{}\"", escape_double_quoted(trimmed))
}

// Converts the selected action + path/custom command into the shell command
// that is written into aliases.zsh.
pub(crate) fn build_command_preview(action: &str, path: &str, custom_command: &str) -> String {
    let path = shell_path(path);
    let with_path = |format: &dyn Fn(&str) -> String| {
        if path.is_empty() {
            String::new()
        } else {
            format(&path)
        }
    };
    match action {
        "navigate" => with_path(&|p| format!("cd {}", p)),
        "open" => with_path(&|p| format!("open {}", p)),
        "execute" => path.clone(),
        "compile_gradle" => with_path(&|p| format!("cd {} && ./gradlew build", p)),
        "compile_maven" => with_path(&|p| format!("cd {} && mvn clean package", p)),
        "custom" => custom_command.trim().to_string(),
        _ => String::new(),
    }
}

// Shared validation for the create and edit forms. Returns the message to show.
pub(crate) fn validate_alias_form(
    name: &str,
    action: &str,
    path: &str,
    custom_command: &str,
) -> Option<String> {
    if !validate_alias_name(name.trim()) {
        return Some(
            "Alias name must start with a letter or _ and may only contain letters, numbers, _ or -."
                .to_string(),
        );
    }
    if name.trim() == APP_ALIAS_NAME {
        return Some(format!(
            "\"{}\" is reserved for the EasyAlias desktop app.",
            APP_ALIAS_NAME
        ));
    }
    if action == "custom" {
        if custom_command.trim().is_empty() {
            return Some("Custom Command cannot be empty.".to_string());
        }
        return None;
    }
    if path.trim().is_empty() {
        return Some("Please enter a path or command.".to_string());
    }
    None
}

// Same shape as the desktop app's timestamps (JavaScript toISOString).
pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

// Random v4-style UUID, matching the ids crypto.randomUUID() creates in the
// desktop app. Falls back to time + counter if /dev/urandom is unavailable.
pub(crate) fn create_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut bytes = [0u8; 16];
    let filled = fs::File::open("/dev/urandom")
        .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut bytes))
        .is_ok();
    if !filled {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
        bytes = (nanos ^ (count << 64) ^ (std::process::id() as u128) << 32).to_le_bytes();
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{:02x}", byte)).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

// "~/x" -> "$HOME/x" for paths the TUI hands to the file system directly.
pub(crate) fn expand_home(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" {
        return home_dir().unwrap_or_else(|_| PathBuf::from(trimmed));
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Ok(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    #[test]
    fn builds_the_same_commands_as_the_desktop_app() {
        assert_eq!(
            build_command_preview("navigate", "~/Projects/app", ""),
            "cd \"$HOME/Projects/app\""
        );
        assert_eq!(build_command_preview("navigate", "~", ""), "cd \"$HOME\"");
        assert_eq!(
            build_command_preview("open", "/tmp/a b.txt", ""),
            "open \"/tmp/a b.txt\""
        );
        assert_eq!(
            build_command_preview("execute", "./run.sh", ""),
            "\"./run.sh\""
        );
        assert_eq!(
            build_command_preview("compile_gradle", "~/app", ""),
            "cd \"$HOME/app\" && ./gradlew build"
        );
        assert_eq!(
            build_command_preview("compile_maven", "/srv/x", ""),
            "cd \"/srv/x\" && mvn clean package"
        );
        assert_eq!(
            build_command_preview("custom", "", "  git status  "),
            "git status"
        );
        assert_eq!(build_command_preview("navigate", "  ", ""), "");
        assert_eq!(
            build_command_preview("open", "/a/$HOME\"`x`", ""),
            "open \"/a/\\$HOME\\\"\\`x\\`\""
        );
    }

    #[test]
    fn validates_like_the_desktop_app() {
        assert!(validate_alias_form("gs", "custom", "", "git status").is_none());
        assert!(validate_alias_form("1gs", "custom", "", "git status").is_some());
        assert!(validate_alias_form("gs", "custom", "", "  ").is_some());
        assert!(validate_alias_form("proj", "navigate", "", "").is_some());
        assert!(validate_alias_form("easya", "navigate", "~", "").is_some());
    }

    #[test]
    fn creates_uuid_shaped_ids() {
        let id = create_id();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert_ne!(id, create_id());
    }
}
