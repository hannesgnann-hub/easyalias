// Alias JSON backup export/import.

import { actionLabels } from "../constants";
import { escapeHtml } from "../html";
import { clearMessages } from "../messages";
import { invokeCommand, isTauriRuntime, nowIso } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { AliasEntry, BackupExportResult, BackupImportResult } from "../types";
import { compareAliases } from "./list";

export function openBackupExport() {
  clearMessages();
  state.backupError = "";
  state.backupFilePath = "";
  state.backupCandidates = [...state.appState.aliases].sort(compareAliases);
  state.selectedBackupIds = new Set(state.backupCandidates.map((alias) => alias.id));
  state.backupDialogMode = "export";
  render();
}

export function openBackupImport() {
  clearMessages();
  state.backupError = "";
  state.backupFilePath = "";
  state.backupCandidates = [];
  state.selectedBackupIds.clear();
  state.backupDialogMode = "import";
  render();
}

export function closeBackupDialog() {
  if (state.backupBusy) return;
  state.backupDialogMode = null;
  state.backupCandidates = [];
  state.selectedBackupIds.clear();
  state.backupFilePath = "";
  state.backupError = "";
  render();
}

export async function chooseBackupFile() {
  if (state.backupBusy || state.backupDialogMode !== "import") return;
  state.backupError = "";

  if (!isTauriRuntime()) {
    state.backupError = "Backup files can only be opened in the Tauri app.";
    render();
    return;
  }

  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "EasyAlias Backup", extensions: ["json"] }]
    });
    if (typeof selected === "string") await inspectBackupFile(selected);
  } catch (fileError) {
    state.backupError = `Backup could not be selected: ${String(fileError)}`;
    render();
  }
}

export async function inspectBackupFile(path: string) {
  if (state.backupBusy || state.backupDialogMode !== "import") return;
  state.backupBusy = true;
  state.backupError = "";
  render();

  try {
    state.backupCandidates = await invokeCommand<AliasEntry[]>("inspect_alias_backup", { path });
    state.backupFilePath = path;
    state.selectedBackupIds = new Set(state.backupCandidates.map((alias) => alias.id));
  } catch (inspectError) {
    state.backupCandidates = [];
    state.selectedBackupIds.clear();
    state.backupFilePath = "";
    state.backupError = String(inspectError);
  }

  state.backupBusy = false;
  render();
}

export async function exportSelectedAliases(event: SubmitEvent) {
  event.preventDefault();
  if (state.backupBusy || state.backupDialogMode !== "export") return;
  state.backupError = "";

  if (state.selectedBackupIds.size === 0) {
    state.backupError = "Select at least one alias to export.";
    render();
    return;
  }
  if (!isTauriRuntime()) {
    state.backupError = "Backups can only be exported in the Tauri app.";
    render();
    return;
  }

  try {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const destination = await save({
      defaultPath: `EasyAlias-backup-${new Date().toISOString().slice(0, 10)}.json`,
      filters: [{ name: "EasyAlias Backup", extensions: ["json"] }]
    });
    if (typeof destination !== "string") return;

    state.backupBusy = true;
    render();
    const result = await invokeCommand<BackupExportResult>("export_alias_backup", {
      selectedIds: [...state.selectedBackupIds],
      destination,
      exportedAt: nowIso()
    });
    closeBackupDialogAfterSuccess();
    state.notice = `${result.exportedCount} aliases exported to ${result.file}.`;
  } catch (exportError) {
    state.backupError = String(exportError);
  }

  state.backupBusy = false;
  render();
}

export async function importSelectedBackupAliases(event: SubmitEvent) {
  event.preventDefault();
  if (state.backupBusy || state.backupDialogMode !== "import") return;
  state.backupError = "";

  if (!state.backupFilePath || state.selectedBackupIds.size === 0) {
    state.backupError = state.backupFilePath
      ? "Select at least one alias to import."
      : "Choose or drop an EasyAlias backup first.";
    render();
    return;
  }

  state.backupBusy = true;
  render();
  try {
    const result = await invokeCommand<BackupImportResult>("import_alias_backup", {
      path: state.backupFilePath,
      selectedIds: [...state.selectedBackupIds],
      importedAt: nowIso()
    });
    state.appState = result.state;
    closeBackupDialogAfterSuccess();
    const replacementNote = result.replacedCount
      ? ` ${result.replacedCount} existing aliases replaced.`
      : "";
    state.notice = `${result.importedCount} aliases imported.${replacementNote}`;
  } catch (backupImportError) {
    state.backupError = String(backupImportError);
  }

  state.backupBusy = false;
  render();
}

export function closeBackupDialogAfterSuccess() {
  state.backupDialogMode = null;
  state.backupCandidates = [];
  state.selectedBackupIds.clear();
  state.backupFilePath = "";
  state.backupError = "";
}

export function renderBackupDialog() {
  if (!state.backupDialogMode) return "";

  const isExport = state.backupDialogMode === "export";
  const allSelected =
    state.backupCandidates.length > 0 &&
    state.backupCandidates.every((alias) => state.selectedBackupIds.has(alias.id));
  const existingNames = new Set(state.appState.aliases.map((alias) => alias.name));
  const fileName = state.backupFilePath.split(/[\\/]/).pop() ?? state.backupFilePath;

  return `
    <section class="modal-layer" role="presentation">
      <form class="modal-card import-card backup-card" id="${isExport ? "backup-export-form" : "backup-import-form"}" role="dialog" aria-modal="true" aria-labelledby="backup-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">${isExport ? "Portable Backup" : "Restore Backup"}</p>
            <h2 id="backup-title">${isExport ? "Export aliases" : "Import aliases"}</h2>
          </div>
          <button class="ghost-button modal-close" type="button" data-action="close-backup" ${state.backupBusy ? "disabled" : ""}>Close</button>
        </div>

        <p class="import-intro">${isExport
          ? "Choose which aliases to include. The resulting JSON file can be restored with EasyAlias later."
          : "Choose an EasyAlias JSON backup or drop it below. You can review every alias before anything is changed."}</p>
        ${state.backupError ? `<p class="modal-error">${escapeHtml(state.backupError)}</p>` : ""}

        ${isExport ? "" : `<button class="backup-drop-zone" type="button" data-action="choose-backup-file" ${state.backupBusy ? "disabled" : ""}>
          <span class="backup-drop-icon" aria-hidden="true">&#8681;</span>
          <strong>${state.backupFilePath ? escapeHtml(fileName) : "Drop an EasyAlias backup here"}</strong>
          <span>${state.backupFilePath ? `${state.backupCandidates.length} aliases found` : "or click to choose a .json file"}</span>
        </button>`}

        ${state.backupCandidates.length ? `<label class="import-select-all">
          <input type="checkbox" name="backup-all" ${allSelected ? "checked" : ""} ${state.backupBusy ? "disabled" : ""} />
          <span>Select all</span>
        </label>
        <div class="import-list" aria-label="Aliases available for ${isExport ? "export" : "import"}">
          ${state.backupCandidates.map((alias) => {
            const willReplace = !isExport && existingNames.has(alias.name);
            return `<label class="import-row">
              <input type="checkbox" name="backup-candidate" value="${escapeHtml(alias.id)}" ${state.selectedBackupIds.has(alias.id) ? "checked" : ""} ${state.backupBusy ? "disabled" : ""} />
              <span class="import-alias-copy">
                <span class="import-alias-meta"><strong>${escapeHtml(alias.name)}</strong><span class="${willReplace ? "backup-conflict" : ""}">${willReplace ? "Replaces existing" : actionLabels[alias.action]}</span></span>
                <code>${escapeHtml(alias.commandPreview)}</code>
              </span>
            </label>`;
          }).join("")}
        </div>` : isExport ? `<p class="backup-empty">No aliases are available to export.</p>` : ""}

        <p class="import-safety">${isExport
          ? "The backup contains only the aliases you select."
          : "Aliases with matching names replace their current EasyAlias entry. Unselected aliases stay unchanged."}</p>
        <div class="modal-actions import-actions">
          <button class="ghost-button" type="button" data-action="close-backup" ${state.backupBusy ? "disabled" : ""}>Cancel</button>
          <button class="primary-button" type="submit" data-backup-submit ${state.selectedBackupIds.size && !state.backupBusy ? "" : "disabled"}>${state.backupBusy ? "Working..." : `${isExport ? "Export" : "Import"} Selected (${state.selectedBackupIds.size})`}</button>
        </div>
      </form>
    </section>`;
}

// Keep backup controls in sync without rebuilding the modal. Re-rendering would
// replace the scroll container and jump the user back to the beginning.
export function syncBackupSelectionControls() {
  const selectedCount = state.backupCandidates.filter((alias) => state.selectedBackupIds.has(alias.id)).length;
  const selectAll = document.querySelector<HTMLInputElement>('input[name="backup-all"]');

  if (selectAll) {
    selectAll.checked = state.backupCandidates.length > 0 && selectedCount === state.backupCandidates.length;
    selectAll.indeterminate = selectedCount > 0 && selectedCount < state.backupCandidates.length;
  }

  const submitButton = document.querySelector<HTMLButtonElement>("[data-backup-submit]");
  if (submitButton) {
    const actionLabel = state.backupDialogMode === "export" ? "Export" : "Import";
    submitButton.disabled = selectedCount === 0 || state.backupBusy;
    submitButton.textContent = state.backupBusy ? "Working..." : `${actionLabel} Selected (${selectedCount})`;
  }
}
