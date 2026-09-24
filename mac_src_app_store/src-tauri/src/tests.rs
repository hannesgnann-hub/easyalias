use super::*;

fn alias(name: &str, command: &str) -> AliasEntry {
    AliasEntry {
        id: name.to_string(),
        name: name.to_string(),
        path: String::new(),
        action: "custom".to_string(),
        custom_command: Some(command.to_string()),
        command_preview: command.to_string(),
        favorite: false,
        created_at: "2026-07-26T12:00:00.000Z".to_string(),
        updated_at: "2026-07-26T12:00:00.000Z".to_string(),
    }
}

#[test]
fn parses_aliases_outside_the_managed_block() {
    let content = format!(
        "alias legacy='echo legacy'\n{}\nalias managed='echo managed'\n{}\n",
        MANAGED_BLOCK_START, MANAGED_BLOCK_END
    );
    let candidates = find_shell_aliases(&content).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "legacy");
}

#[test]
fn updates_one_managed_block_without_touching_other_lines() {
    let original = "export PATH=/opt/bin:$PATH\n";
    let first = update_managed_block(original, &[alias("ll", "ls -lah")]).unwrap();
    let second = update_managed_block(&first, &[alias("gs", "git status")]).unwrap();

    assert!(second.starts_with(original));
    assert_eq!(second.matches(MANAGED_BLOCK_START).count(), 1);
    assert!(!second.contains("alias ll="));
    assert!(second.contains("alias gs='git status'"));
}

#[test]
fn rejects_ambiguous_managed_markers() {
    let malformed = format!("{MANAGED_BLOCK_START}\nalias ll='ls'\n");
    assert!(without_managed_block(&malformed).is_err());
}

#[test]
fn replaces_only_confirmed_import_lines() {
    let content = "alias ll='ls -lah'\nalias gs='git status'\n";
    let selected = HashMap::from([(2, "gs")]);

    assert_eq!(
        replace_imported_alias_lines(content, &selected),
        "alias ll='ls -lah'\n: # EasyAlias imported alias gs\n"
    );
}

#[test]
fn old_alias_json_defaults_to_not_favorite() {
    let legacy = r#"{
        "id":"legacy-1",
        "name":"ll",
        "path":"",
        "action":"custom",
        "customCommand":"ls -lah",
        "commandPreview":"ls -lah",
        "createdAt":"2026-07-01T12:00:00.000Z",
        "updatedAt":"2026-07-01T12:00:00.000Z"
    }"#;

    let alias: AliasEntry = serde_json::from_str(legacy).unwrap();
    assert!(!alias.favorite);
}

#[test]
fn expired_trash_entries_are_removed_automatically() {
    let now: u64 = 10_000_000;
    let mut trash = vec![
        TrashEntry {
            alias: alias("old", "echo old"),
            deleted_at: now.saturating_sub(TRASH_RETENTION_SECONDS + 1),
        },
        TrashEntry {
            alias: alias("new", "echo new"),
            deleted_at: now,
        },
    ];

    assert!(retain_current_trash_entries(&mut trash, now));
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].alias.id, "new");
}

#[test]
fn reads_versioned_portable_backup() {
    let path = std::env::temp_dir().join(format!(
        "easyalias-store-backup-test-{}.json",
        unix_timestamp().unwrap()
    ));
    let backup = AliasBackup {
        format: BACKUP_FORMAT.to_string(),
        version: BACKUP_VERSION,
        exported_at: "2026-08-14T12:00:00.000Z".to_string(),
        aliases: vec![alias("ll", "ls -lah")],
    };
    fs::write(&path, serde_json::to_string(&backup).unwrap()).unwrap();

    let restored = read_backup(&path).unwrap();
    let _ = fs::remove_file(&path);

    assert_eq!(restored.aliases.len(), 1);
    assert_eq!(restored.aliases[0].name, "ll");
}
