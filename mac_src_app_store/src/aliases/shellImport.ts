// Importing existing aliases from shell startup files.

import { chooseHomeFolder } from "../homeAccess";
import { escapeHtml } from "../html";
import { clearMessages } from "../messages";
import { invokeCommand, nowIso } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { AppState, ImportResult } from "../types";

// The header import button requests a fresh backend scan even after the
// first-start prompt was handled. Candidates already managed by EasyAlias are
// filtered by Rust before this shared import modal is opened.
export async function openShellImport() {
  if (state.importBusy) return;

  if (!state.appState.homeConnected) {
    await chooseHomeFolder();
    return;
  }

  clearMessages();
  state.importError = "";
  state.importBusy = true;
  render();

  try {
    state.appState = await invokeCommand<AppState>("scan_shell_import");
    state.selectedImportIds = new Set(state.appState.importCandidates.map((candidate) => candidate.id));
    state.manualImportOpen = state.appState.importCandidates.length > 0;

    if (!state.manualImportOpen) {
      state.notice = "No new aliases found in .zshrc, .bash_profile, or .bashrc.";
    }
  } catch (scanError) {
    state.error = String(scanError);
  }

  state.importBusy = false;
  render();
}

export function closeManualImport() {
  if (state.importBusy) return;
  state.appState = { ...state.appState, importCandidates: [] };
  state.selectedImportIds.clear();
  state.importError = "";
  state.manualImportOpen = false;
  render();
}

// Skipping writes a small marker inside the app container so the first-run
// question is not shown again. It never changes an existing alias line.
export async function dismissShellImport() {
  if (state.importBusy) return;
  state.importBusy = true;
  state.importError = "";
  render();

  try {
    state.appState = await invokeCommand<AppState>("dismiss_shell_import");
    state.selectedImportIds.clear();
    state.manualImportOpen = false;
    state.notice = "Existing aliases were left unchanged in your shell files.";
  } catch (dismissError) {
    state.importError = String(dismissError);
  }

  state.importBusy = false;
  render();
}

// The Rust command rescans all allowlisted shell files through the Home-folder
// bookmark, creates backups, and moves only selected lines into managed blocks.
export async function importSelectedShellAliases(event: SubmitEvent) {
  event.preventDefault();
  if (state.importBusy) return;
  state.importError = "";

  if (state.selectedImportIds.size === 0) {
    state.importError = "Select at least one alias to import.";
    render();
    return;
  }

  state.importBusy = true;
  render();

  try {
    const result = await invokeCommand<ImportResult>("import_shell_aliases", {
      selectedIds: [...state.selectedImportIds],
      timestamp: nowIso()
    });
    state.appState = result.state;
    state.selectedImportIds.clear();
    state.manualImportOpen = false;
    state.notice = `${result.importedCount} aliases imported. Backup: ${result.backupFile}`;
  } catch (importFailure) {
    state.importError = String(importFailure);
  }

  state.importBusy = false;
  render();
}

// The same migration dialog handles both first-start discovery and a manual
// rescan from the header. The mode only changes labels and close behavior.
export function renderImportModal() {
  const candidates = state.appState.importCandidates;
  if (!candidates.length) return "";

  const allSelected = candidates.every((candidate) => state.selectedImportIds.has(candidate.id));

  return `
    <section class="modal-layer" role="presentation">
      <form class="modal-card import-card" id="import-form" role="dialog" aria-modal="true" aria-labelledby="import-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">${state.manualImportOpen ? "Import Aliases" : "First Start"}</p>
            <h2 id="import-title">Existing aliases found</h2>
          </div>
          <span class="import-count">${candidates.length} found</span>
        </div>

        <p class="import-intro">
          Select the aliases EasyAlias should manage. Imported entries become Custom Commands and are removed from their original lines only after a backup is created.
        </p>

        ${state.importError ? `<p class="modal-error">${escapeHtml(state.importError)}</p>` : ""}

        <label class="import-select-all">
          <input type="checkbox" name="import-all" ${allSelected ? "checked" : ""} ${state.importBusy ? "disabled" : ""} />
          <span>Select all</span>
        </label>

        <div class="import-list" aria-label="Aliases available for import">
          ${candidates
            .map(
              (candidate) => `
                <label class="import-row">
                  <input
                    type="checkbox"
                    name="import-candidate"
                    value="${escapeHtml(candidate.id)}"
                    ${state.selectedImportIds.has(candidate.id) ? "checked" : ""}
                    ${state.importBusy ? "disabled" : ""}
                  />
                  <span class="import-alias-copy">
                    <span class="import-alias-meta">
                      <strong>${escapeHtml(candidate.name)}</strong>
                      <span>${escapeHtml(candidate.sourceFile)}, line ${candidate.lineNumber}</span>
                    </span>
                    <code>${escapeHtml(candidate.command)}</code>
                  </span>
                </label>
              `
            )
            .join("")}
        </div>

        <p class="import-safety">
          EasyAlias stores timestamped backups inside its App Sandbox container before changing selected shell files.
        </p>

        <div class="modal-actions import-actions">
          <button class="ghost-button" type="button" data-action="${state.manualImportOpen ? "close-import" : "dismiss-import"}" ${state.importBusy ? "disabled" : ""}>${state.manualImportOpen ? "Close" : "Skip Import"}</button>
          <button class="primary-button" type="submit" ${state.selectedImportIds.size && !state.importBusy ? "" : "disabled"}>
            ${state.importBusy ? "Working..." : `Import Selected (${state.selectedImportIds.size})`}
          </button>
        </div>
      </form>
    </section>
  `;
}
