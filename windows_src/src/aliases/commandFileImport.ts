// Importing existing .cmd shortcut files.

import { clearMessages } from "../messages";
import { invokeCommand, nowIso } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { AppState, ImportResult } from "../types";

// Rescan legacy .cmd/.bat aliases even after the first-start prompt was handled.
// Rust filters managed names and remains authoritative for PATH and file access.
export async function openCommandFileImport() {
  if (state.importBusy) return;
  clearMessages();
  state.importError = "";
  state.importBusy = true;
  render();

  try {
    state.appState = await invokeCommand<AppState>("scan_command_file_import");
    state.selectedImportIds = new Set(state.appState.importCandidates.map((candidate) => candidate.id));
    state.manualImportOpen = state.appState.importCandidates.length > 0;

    if (!state.manualImportOpen) {
      state.notice = "No new command files found in your PATH folders.";
    }
  } catch (scanError) {
    state.error = String(scanError);
  }

  state.importBusy = false;
  render();
}

export async function dismissCommandFileImport() {
  if (state.importBusy) return;
  state.importBusy = true;
  state.importError = "";
  render();

  try {
    state.appState = await invokeCommand<AppState>("dismiss_command_file_import");
    state.selectedImportIds.clear();
    state.manualImportOpen = false;
    state.notice = "Existing command files were left unchanged.";
  } catch (dismissError) {
    state.importError = String(dismissError);
  }

  state.importBusy = false;
  render();
}

export async function importSelectedCommandFiles(event: SubmitEvent) {
  event.preventDefault();
  if (state.importBusy) return;
  state.importError = "";

  if (state.selectedImportIds.size === 0) {
    state.importError = "Select at least one command file to import.";
    render();
    return;
  }

  state.importBusy = true;
  render();

  try {
    const result = await invokeCommand<ImportResult>("import_command_files", {
      selectedIds: [...state.selectedImportIds],
      timestamp: nowIso()
    });
    state.appState = result.state;
    state.selectedImportIds.clear();
    state.manualImportOpen = false;
    state.notice = `${result.importedCount} command files imported. Backup: ${result.backupDir}`;
    if (result.warning) state.error = result.warning;
  } catch (importFailure) {
    state.importError = String(importFailure);
  }

  state.importBusy = false;
  render();
}
