// Importing existing aliases from shell startup files.

import { escapeHtml } from "../html";
import { render } from "../render";
import { state } from "../state";

export function closeManualImport() {
  if (state.importBusy) return;
  state.appState = { ...state.appState, importCandidates: [] };
  state.selectedImportIds.clear();
  state.importError = "";
  state.manualImportOpen = false;
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
            <h2 id="import-title">Existing command files found</h2>
          </div>
          <span class="import-count">${candidates.length} found</span>
        </div>

        <p class="import-intro">
          EasyAlias found simple .cmd or .bat aliases in user-owned PATH folders. Selected files become Custom Commands and move only after a backup is created.
        </p>

        ${state.importError ? `<p class="modal-error">${escapeHtml(state.importError)}</p>` : ""}

        <label class="import-select-all">
          <input type="checkbox" name="import-all" ${allSelected ? "checked" : ""} ${state.importBusy ? "disabled" : ""} />
          <span>Select all</span>
        </label>

        <div class="import-list" aria-label="Command files available for import">
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
                      <span>${escapeHtml(candidate.sourceFile)}</span>
                    </span>
                    <code>${escapeHtml(candidate.command)}</code>
                  </span>
                </label>
              `
            )
            .join("")}
        </div>

        <p class="import-safety">
          Original files are copied to a timestamped <code>~/.easyalias/import-backup-*</code> folder before they are removed from the old PATH folder.
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
