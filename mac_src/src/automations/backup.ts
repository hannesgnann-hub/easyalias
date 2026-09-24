// Automation JSON backup export/import.

import { escapeHtml } from "../html";
import { clearMessages } from "../messages";
import { invokeCommand, isTauriRuntime, nowIso } from "../platform";
import { state } from "../state";
import type { Automation, AutomationBackupImportResult, BackupExportResult } from "../types";
import { compareAutomations } from "./filters";
import { renderAutomationsView } from "./view";

export function openAutomationBackupExport() {
  clearMessages();
  state.automationBackupError = "";
  state.automationBackupFilePath = "";
  state.automationBackupCandidates = [...state.automations].sort(compareAutomations);
  state.selectedAutomationBackupIds = new Set(state.automationBackupCandidates.map((automation) => automation.id));
  state.automationBackupDialogMode = "export";
  renderAutomationsView();
}

export function openAutomationBackupImport() {
  clearMessages();
  state.automationBackupError = "";
  state.automationBackupFilePath = "";
  state.automationBackupCandidates = [];
  state.selectedAutomationBackupIds.clear();
  state.automationBackupDialogMode = "import";
  renderAutomationsView();
}

export function closeAutomationBackupDialog() {
  if (state.automationBackupBusy) return;
  state.automationBackupDialogMode = null;
  state.automationBackupCandidates = [];
  state.selectedAutomationBackupIds.clear();
  state.automationBackupFilePath = "";
  state.automationBackupError = "";
  renderAutomationsView();
}

export function closeAutomationBackupDialogAfterSuccess() {
  state.automationBackupDialogMode = null;
  state.automationBackupCandidates = [];
  state.selectedAutomationBackupIds.clear();
  state.automationBackupFilePath = "";
  state.automationBackupError = "";
}

export async function chooseAutomationBackupFile() {
  if (state.automationBackupBusy || state.automationBackupDialogMode !== "import") return;
  state.automationBackupError = "";

  if (!isTauriRuntime()) {
    state.automationBackupError = "Automation backup files can only be opened in the Tauri app.";
    renderAutomationsView();
    return;
  }

  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "EasyAlias Automation Backup", extensions: ["json"] }]
    });
    if (typeof selected === "string") await inspectAutomationBackupFile(selected);
  } catch (fileError) {
    state.automationBackupError = `Automation backup could not be selected: ${String(fileError)}`;
    renderAutomationsView();
  }
}

export async function inspectAutomationBackupFile(path: string) {
  if (state.automationBackupBusy || state.automationBackupDialogMode !== "import") return;
  state.automationBackupBusy = true;
  state.automationBackupError = "";
  renderAutomationsView();

  try {
    state.automationBackupCandidates = await invokeCommand<Automation[]>("inspect_automation_backup", { path });
    state.automationBackupFilePath = path;
    state.selectedAutomationBackupIds = new Set(state.automationBackupCandidates.map((automation) => automation.id));
  } catch (inspectError) {
    state.automationBackupCandidates = [];
    state.selectedAutomationBackupIds.clear();
    state.automationBackupFilePath = "";
    state.automationBackupError = String(inspectError);
  }

  state.automationBackupBusy = false;
  renderAutomationsView();
}

export async function exportSelectedAutomations(event: SubmitEvent) {
  event.preventDefault();
  if (state.automationBackupBusy || state.automationBackupDialogMode !== "export") return;
  state.automationBackupError = "";

  if (state.selectedAutomationBackupIds.size === 0) {
    state.automationBackupError = "Select at least one automation to export.";
    renderAutomationsView();
    return;
  }
  if (!isTauriRuntime()) {
    state.automationBackupError = "Automation backups can only be exported in the Tauri app.";
    renderAutomationsView();
    return;
  }

  try {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const destination = await save({
      defaultPath: `EasyAlias-automations-backup-${new Date().toISOString().slice(0, 10)}.json`,
      filters: [{ name: "EasyAlias Automation Backup", extensions: ["json"] }]
    });
    if (typeof destination !== "string") return;

    state.automationBackupBusy = true;
    renderAutomationsView();
    const result = await invokeCommand<BackupExportResult>("export_automation_backup", {
      selectedIds: [...state.selectedAutomationBackupIds],
      destination,
      exportedAt: nowIso()
    });
    closeAutomationBackupDialogAfterSuccess();
    state.notice = `${result.exportedCount} automations exported to ${result.file}.`;
  } catch (exportError) {
    state.automationBackupError = String(exportError);
  }

  state.automationBackupBusy = false;
  renderAutomationsView();
}

export async function importSelectedBackupAutomations(event: SubmitEvent) {
  event.preventDefault();
  if (state.automationBackupBusy || state.automationBackupDialogMode !== "import") return;
  state.automationBackupError = "";

  if (!state.automationBackupFilePath || state.selectedAutomationBackupIds.size === 0) {
    state.automationBackupError = state.automationBackupFilePath
      ? "Select at least one automation to import."
      : "Choose or drop an EasyAlias automation backup first.";
    renderAutomationsView();
    return;
  }

  state.automationBackupBusy = true;
  renderAutomationsView();
  try {
    const result = await invokeCommand<AutomationBackupImportResult>("import_automation_backup", {
      path: state.automationBackupFilePath,
      selectedIds: [...state.selectedAutomationBackupIds],
      importedAt: nowIso()
    });
    state.automations = result.automations;
    closeAutomationBackupDialogAfterSuccess();
    const replacementNote = result.replacedCount
      ? ` ${result.replacedCount} existing automations replaced.`
      : "";
    state.notice = `${result.importedCount} automations imported.${replacementNote}`;
  } catch (backupImportError) {
    state.automationBackupError = String(backupImportError);
  }

  state.automationBackupBusy = false;
  renderAutomationsView();
}

export function renderAutomationBackupDialog() {
  if (!state.automationBackupDialogMode) return "";

  const isExport = state.automationBackupDialogMode === "export";
  const allSelected =
    state.automationBackupCandidates.length > 0 &&
    state.automationBackupCandidates.every((automation) => state.selectedAutomationBackupIds.has(automation.id));
  const existingNames = new Set(state.automations.map((automation) => automation.name));
  const fileName = state.automationBackupFilePath.split(/[\\/]/).pop() ?? state.automationBackupFilePath;

  return `
    <section class="modal-layer" role="presentation">
      <form
        class="modal-card import-card backup-card"
        id="${isExport ? "automation-backup-export-form" : "automation-backup-import-form"}"
        role="dialog"
        aria-modal="true"
        aria-labelledby="automation-backup-title"
      >
        <div class="modal-title">
          <div>
            <p class="eyebrow">${isExport ? "Portable Workflows" : "Restore Workflows"}</p>
            <h2 id="automation-backup-title">${isExport ? "Export automations" : "Import automations"}</h2>
          </div>
          <button class="ghost-button modal-close" type="button" data-automation-action="close-backup" ${state.automationBackupBusy ? "disabled" : ""}>Close</button>
        </div>

        <p class="import-intro">
          ${
            isExport
              ? "Choose which automations to include. The versioned JSON backup keeps every path, step, behavior, and favorite state."
              : "Choose an EasyAlias automation backup or drop it below. You can review every workflow before anything is changed."
          }
        </p>

        ${state.automationBackupError ? `<p class="modal-error">${escapeHtml(state.automationBackupError)}</p>` : ""}

        ${
          isExport
            ? ""
            : `<button class="backup-drop-zone automation-backup-drop-zone" type="button" data-automation-action="choose-backup-file" ${state.automationBackupBusy ? "disabled" : ""}>
                <span class="backup-drop-icon" aria-hidden="true">&#8681;</span>
                <strong>${state.automationBackupFilePath ? escapeHtml(fileName) : "Drop an automation backup here"}</strong>
                <span>${state.automationBackupFilePath ? `${state.automationBackupCandidates.length} automations found` : "or click to choose a .json file"}</span>
              </button>`
        }

        ${
          state.automationBackupCandidates.length
            ? `<label class="import-select-all">
                <input type="checkbox" name="automation-backup-all" ${allSelected ? "checked" : ""} ${state.automationBackupBusy ? "disabled" : ""} />
                <span>Select all</span>
              </label>

              <div class="import-list" aria-label="Automations available for ${isExport ? "export" : "import"}">
                ${state.automationBackupCandidates
                  .map((automation) => {
                    const willReplace = !isExport && existingNames.has(automation.name);
                    const stepLabel = `${automation.steps.length} ${automation.steps.length === 1 ? "step" : "steps"}`;
                    return `
                      <label class="import-row">
                        <input
                          type="checkbox"
                          name="automation-backup-candidate"
                          value="${escapeHtml(automation.id)}"
                          ${state.selectedAutomationBackupIds.has(automation.id) ? "checked" : ""}
                          ${state.automationBackupBusy ? "disabled" : ""}
                        />
                        <span class="import-alias-copy">
                          <span class="import-alias-meta">
                            <strong>${escapeHtml(automation.name)}</strong>
                            <span class="${willReplace ? "backup-conflict" : ""}">${willReplace ? "Replaces existing" : stepLabel}</span>
                          </span>
                          <code>${escapeHtml(automation.path)}</code>
                        </span>
                      </label>`;
                  })
                  .join("")}
              </div>`
            : isExport
              ? `<p class="backup-empty">No automations are available to export.</p>`
              : ""
        }

        <p class="import-safety">
          ${
            isExport
              ? "The backup contains only the automations you select."
              : "Automations with matching names replace their current EasyAlias workflow. Unselected automations stay unchanged."
          }
        </p>

        <div class="modal-actions import-actions">
          <button class="ghost-button" type="button" data-automation-action="close-backup" ${state.automationBackupBusy ? "disabled" : ""}>Cancel</button>
          <button class="primary-button" type="submit" data-automation-backup-submit ${state.selectedAutomationBackupIds.size && !state.automationBackupBusy ? "" : "disabled"}>
            ${state.automationBackupBusy ? "Working..." : `${isExport ? "Export" : "Import"} Selected (${state.selectedAutomationBackupIds.size})`}
          </button>
        </div>
      </form>
    </section>`;
}

// Automation backups use the same no-re-render selection behavior as alias
// backups so long lists keep their current scroll position while selecting.
export function syncAutomationBackupSelectionControls() {
  const selectedCount = state.automationBackupCandidates.filter((automation) =>
    state.selectedAutomationBackupIds.has(automation.id),
  ).length;
  const selectAll = document.querySelector<HTMLInputElement>('input[name="automation-backup-all"]');

  if (selectAll) {
    selectAll.checked =
      state.automationBackupCandidates.length > 0 && selectedCount === state.automationBackupCandidates.length;
    selectAll.indeterminate = selectedCount > 0 && selectedCount < state.automationBackupCandidates.length;
  }

  const submitButton = document.querySelector<HTMLButtonElement>("[data-automation-backup-submit]");
  if (submitButton) {
    const actionLabel = state.automationBackupDialogMode === "export" ? "Export" : "Import";
    submitButton.disabled = selectedCount === 0 || state.automationBackupBusy;
    submitButton.textContent = state.automationBackupBusy
      ? "Working..."
      : `${actionLabel} Selected (${selectedCount})`;
  }
}
