//! Packages from the EasyAlias marketplace (`*.easyaliaspack.json`).
//!
//! A package is imported through the same dialogs as a backup: the alias import
//! takes its aliases, the automation import its automations. This module turns a
//! package into the regular backup structures, so everything after reading the
//! file - review, selection, name conflicts, writing - is the backup code.
//!
//! Packages come from other people, so nothing in them is trusted blindly:
//! - an alias's command is rebuilt from action + path/customCommand; the
//!   package's own `commandPreview` is ignored, so the dialog shows exactly what
//!   ends up in aliases.sh,
//! - automations never bring a global hotkey,
//! - ids and timestamps are assigned here.

use crate::*;

pub(crate) const PACKAGE_FORMAT: &str = "easyalias-marketplace-package";
pub(crate) const PACKAGE_VERSION: u32 = 1;
const THIS_PLATFORM: &str = "linux";

#[derive(Debug, Deserialize)]
pub(crate) struct FormatProbe {
    #[serde(default)]
    pub(crate) format: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MarketplacePackage {
    pub(crate) format: String,
    pub(crate) version: u32,
    pub(crate) package: PackageInfo,
    #[serde(default)]
    pub(crate) items: PackageItems,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageInfo {
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) slug: String,
    #[serde(default)]
    pub(crate) platforms: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageItems {
    #[serde(default)]
    pub(crate) aliases: Vec<PackageAlias>,
    #[serde(default)]
    pub(crate) automations: Vec<PackageAutomation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageAlias {
    pub(crate) name: String,
    #[serde(default = "default_package_action")]
    pub(crate) action: String,
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) custom_command: Option<String>,
    // Only used as the command of a custom alias that has no customCommand.
    #[serde(default)]
    pub(crate) command_preview: Option<String>,
    #[serde(default)]
    pub(crate) favorite: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageAutomation {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) path: String,
    pub(crate) steps: Vec<PackageStep>,
    #[serde(default)]
    pub(crate) favorite: bool,
    #[serde(default)]
    pub(crate) group: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageStep {
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) seconds: u64,
    #[serde(default = "default_command_behavior")]
    pub(crate) behavior: String,
}

fn default_package_action() -> String {
    "custom".to_string()
}

pub(crate) fn is_package(content: &str) -> bool {
    serde_json::from_str::<FormatProbe>(content)
        .map(|probe| probe.format == PACKAGE_FORMAT)
        .unwrap_or(false)
}

pub(crate) fn parse_package(content: &str) -> Result<MarketplacePackage, String> {
    let package: MarketplacePackage = serde_json::from_str(content)
        .map_err(|error| format!("This is not a valid EasyAlias package: {}", error))?;
    if package.format != PACKAGE_FORMAT {
        return Err("This is not an EasyAlias package.".to_string());
    }
    if package.version != PACKAGE_VERSION {
        return Err("This package needs a newer version of EasyAlias.".to_string());
    }
    let platforms: Vec<String> = package
        .package
        .platforms
        .iter()
        .map(|platform| platform.trim().to_lowercase())
        .collect();
    if !platforms.is_empty() && !platforms.iter().any(|platform| platform == THIS_PLATFORM) {
        return Err(format!(
            "\"{}\" is not made for Linux.",
            package_title(&package)
        ));
    }
    Ok(package)
}

fn package_title(package: &MarketplacePackage) -> String {
    let title = package.package.title.trim();
    if title.is_empty() {
        "This package".to_string()
    } else {
        title.to_string()
    }
}

// Ids are derived from the package, so the dialog and the later import (which
// reads the file again) agree on what was selected.
fn id_prefix(package: &MarketplacePackage) -> String {
    let slug: String = package
        .package
        .slug
        .trim()
        .to_lowercase()
        .chars()
        .filter(|char| char.is_ascii_alphanumeric() || *char == '-')
        .take(60)
        .collect();
    if slug.is_empty() {
        "package".to_string()
    } else {
        format!("package-{}", slug)
    }
}

fn import_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

// ----- aliases ----------------------------------------------------------------

// Escape characters that can break a double-quoted shell string.
fn escape_double_quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
}

// Same rules as `shellPath` in src/aliases/command.ts.
fn shell_path(path: &str) -> String {
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

// Same rules as `buildCommandPreview` in src/aliases/command.ts.
pub(crate) fn alias_command(action: &str, path: &str, custom_command: &str) -> String {
    let path = shell_path(path);
    let with_path = |command: String| {
        if path.is_empty() {
            String::new()
        } else {
            command
        }
    };
    match action {
        "navigate" => with_path(format!("cd {}", path)),
        "open" => with_path(format!("xdg-open {}", path)),
        "execute" => path.clone(),
        "compile_gradle" => with_path(format!("cd {} && ./gradlew build", path)),
        "compile_maven" => with_path(format!("cd {} && mvn clean package", path)),
        "custom" => custom_command.trim().to_string(),
        _ => String::new(),
    }
}

pub(crate) fn package_aliases(content: &str) -> Result<AliasBackup, String> {
    let package = parse_package(content)?;
    if package.items.aliases.is_empty() {
        let automations = package.items.automations.len();
        return Err(if automations > 0 {
            format!(
                "{} has no aliases, only {} automation{}. Import it under Automations.",
                package_title(&package),
                automations,
                if automations == 1 { "" } else { "s" }
            )
        } else {
            format!("{} has no aliases.", package_title(&package))
        });
    }

    let prefix = id_prefix(&package);
    let now = import_timestamp();
    let aliases = package
        .items
        .aliases
        .iter()
        .enumerate()
        .map(|(index, alias)| {
            let name = alias.name.trim().to_string();
            let custom = alias
                .custom_command
                .clone()
                .filter(|command| !command.trim().is_empty())
                .or_else(|| alias.command_preview.clone())
                .unwrap_or_default();
            let command = alias_command(&alias.action, &alias.path, &custom);
            if command.is_empty() {
                return Err(format!(
                    "Alias \"{}\" in this package has no command.",
                    name
                ));
            }
            Ok(AliasEntry {
                id: format!("{}-alias-{}", prefix, index),
                name,
                path: alias.path.trim().to_string(),
                action: alias.action.clone(),
                custom_command: (alias.action == "custom").then(|| custom.trim().to_string()),
                command_preview: command,
                favorite: alias.favorite,
                created_at: now.clone(),
                updated_at: now.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    validate_alias_collection(&aliases)?;

    Ok(AliasBackup {
        format: BACKUP_FORMAT.to_string(),
        version: BACKUP_VERSION,
        exported_at: now,
        aliases,
    })
}

// ----- automations ------------------------------------------------------------

pub(crate) fn package_automations(content: &str) -> Result<AutomationBackup, String> {
    let package = parse_package(content)?;
    if package.items.automations.is_empty() {
        let aliases = package.items.aliases.len();
        return Err(if aliases > 0 {
            format!(
                "{} has no automations, only {} alias{}. Import it under Aliases.",
                package_title(&package),
                aliases,
                if aliases == 1 { "" } else { "es" }
            )
        } else {
            format!("{} has no automations.", package_title(&package))
        });
    }

    let prefix = id_prefix(&package);
    let now = import_timestamp();
    let automations: Vec<Automation> = package
        .items
        .automations
        .iter()
        .enumerate()
        .map(|(index, automation)| {
            let path = automation.path.trim();
            Automation {
                id: format!("{}-automation-{}", prefix, index),
                name: automation.name.trim().to_string(),
                // An automation without a folder starts in the home folder.
                path: if path.is_empty() {
                    "~".to_string()
                } else {
                    path.to_string()
                },
                steps: automation
                    .steps
                    .iter()
                    .enumerate()
                    .map(|(step_index, step)| AutomationStep {
                        id: format!("step-{}", step_index),
                        kind: step.kind.clone(),
                        command: step.command.clone(),
                        seconds: step.seconds,
                        behavior: step.behavior.clone(),
                    })
                    .collect(),
                favorite: automation.favorite,
                group: automation.group.trim().to_string(),
                hotkey: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            }
        })
        .collect();
    validate_automation_backup_collection(&automations)?;

    Ok(AutomationBackup {
        format: AUTOMATION_BACKUP_FORMAT.to_string(),
        version: AUTOMATION_BACKUP_VERSION,
        exported_at: now,
        automations,
    })
}

// ----- the part the other dialog imports --------------------------------------

// After importing a package's aliases, a hint that it also has automations
// (and the other way round). Backups never get a hint.
pub(crate) fn remaining_items_note(path: &Path, imported_aliases: bool) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    if !is_package(&content) {
        return None;
    }
    let package = parse_package(&content).ok()?;
    let (count, singular, plural, place) = if imported_aliases {
        (
            package.items.automations.len(),
            "automation",
            "automations",
            "Automations",
        )
    } else {
        (package.items.aliases.len(), "alias", "aliases", "Aliases")
    };
    (count > 0).then(|| {
        format!(
            "The file also contains {} {} - import it under {}.",
            count,
            if count == 1 { singular } else { plural },
            place
        )
    })
}
