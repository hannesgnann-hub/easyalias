// DOM event wiring for the alias view.

import {
  chooseBackupFile,
  closeBackupDialog,
  exportSelectedAliases,
  importSelectedBackupAliases,
  openBackupExport,
  openBackupImport,
  syncBackupSelectionControls
} from "./aliases/backup";
import { closeEditModal, openEditModal, updateAlias, updateEditForm } from "./aliases/edit";
import {
  deleteAlias,
  showSuggestionPage,
  toggleFavorite,
  toggleSuggestions,
  updateForm,
  upsertAlias,
  useSuggestion
} from "./aliases/list";
import {
  closeManualImport,
  dismissShellImport,
  importSelectedShellAliases,
  openShellImport
} from "./aliases/shellImport";
import {
  closeTrash,
  emptyTrash,
  openTrash,
  permanentlyDeleteTrashAlias,
  restoreTrashAlias
} from "./aliases/trash";
import { dismissMessage } from "./messages";
import { openAutomationsView, openSettingsView } from "./navigation";
import { openExternalLink, openPathPicker } from "./platform";
import { render } from "./render";
import { state } from "./state";
import { bindTutorialEvents, openTutorial } from "./tutorial";
import type { AliasAction } from "./types";

// Because render() replaces the DOM, event listeners are reattached after every render.
// Small live-preview updates skip render(), so their listeners stay intact.
export function bindEvents() {
  document.querySelector<HTMLFormElement>("#alias-form")?.addEventListener("submit", upsertAlias);
  document.querySelector<HTMLFormElement>("#edit-form")?.addEventListener("submit", updateAlias);
  document.querySelector<HTMLFormElement>("#import-form")?.addEventListener("submit", importSelectedShellAliases);
  document.querySelector<HTMLFormElement>("#backup-export-form")?.addEventListener("submit", exportSelectedAliases);
  document.querySelector<HTMLFormElement>("#backup-import-form")?.addEventListener("submit", importSelectedBackupAliases);
  document.querySelectorAll<HTMLAnchorElement>("[data-external-link]").forEach((link) => {
    link.addEventListener("click", openExternalLink);
  });

  document.querySelector<HTMLInputElement>('input[name="name"]')?.addEventListener("input", (event) => {
    updateForm("name", (event.target as HTMLInputElement).value);
  });

  document.querySelector<HTMLInputElement>('input[name="path"]')?.addEventListener("input", (event) => {
    updateForm("path", (event.target as HTMLInputElement).value);
  });

  document.querySelector<HTMLSelectElement>('select[name="action"]')?.addEventListener("change", (event) => {
    updateForm("action", (event.target as HTMLSelectElement).value as AliasAction, true);
  });

  document.querySelector<HTMLTextAreaElement>('textarea[name="customCommand"]')?.addEventListener("input", (event) => {
    updateForm("customCommand", (event.target as HTMLTextAreaElement).value);
  });

  document.querySelector<HTMLInputElement>('input[name="edit-name"]')?.addEventListener("input", (event) => {
    updateEditForm("name", (event.target as HTMLInputElement).value);
  });

  document.querySelector<HTMLInputElement>('input[name="edit-path"]')?.addEventListener("input", (event) => {
    updateEditForm("path", (event.target as HTMLInputElement).value);
  });

  document.querySelector<HTMLSelectElement>('select[name="edit-action"]')?.addEventListener("change", (event) => {
    updateEditForm("action", (event.target as HTMLSelectElement).value as AliasAction, true);
  });

  document.querySelector<HTMLTextAreaElement>('textarea[name="edit-customCommand"]')?.addEventListener("input", (event) => {
    updateEditForm("customCommand", (event.target as HTMLTextAreaElement).value);
  });

  document.querySelector<HTMLInputElement>('input[name="import-all"]')?.addEventListener("change", (event) => {
    state.selectedImportIds = (event.target as HTMLInputElement).checked
      ? new Set(state.appState.importCandidates.map((candidate) => candidate.id))
      : new Set();
    render();
  });

  document.querySelectorAll<HTMLInputElement>('input[name="import-candidate"]').forEach((checkbox) => {
    checkbox.addEventListener("change", () => {
      if (checkbox.checked) state.selectedImportIds.add(checkbox.value);
      else state.selectedImportIds.delete(checkbox.value);
      render();
    });
  });

  document.querySelector<HTMLInputElement>('input[name="backup-all"]')?.addEventListener("change", (event) => {
    const checked = (event.target as HTMLInputElement).checked;
    state.selectedBackupIds = checked
      ? new Set(state.backupCandidates.map((alias) => alias.id))
      : new Set();

    document.querySelectorAll<HTMLInputElement>('input[name="backup-candidate"]').forEach((checkbox) => {
      checkbox.checked = checked;
    });
    syncBackupSelectionControls();
  });

  document.querySelectorAll<HTMLInputElement>('input[name="backup-candidate"]').forEach((checkbox) => {
    checkbox.addEventListener("change", () => {
      if (checkbox.checked) state.selectedBackupIds.add(checkbox.value);
      else state.selectedBackupIds.delete(checkbox.value);
      syncBackupSelectionControls();
    });
  });

  if (state.backupDialogMode) {
    syncBackupSelectionControls();
  }

  document.querySelectorAll<HTMLButtonElement>("[data-action]").forEach((button) => {
    button.addEventListener("click", () => {
      const action = button.dataset.action;
      const id = button.dataset.id;

      if (action === "open-automations") openAutomationsView();
      if (action === "open-settings") openSettingsView();
      if (action === "open-tutorial") openTutorial();
      if (action === "open-import") void openShellImport();
      if (action === "open-backup-export") openBackupExport();
      if (action === "open-backup-import") openBackupImport();
      if (action === "open-trash") void openTrash();
      if (action === "close-trash") closeTrash();
      if (action === "restore-trash" && id) void restoreTrashAlias(id);
      if (action === "permanently-delete-trash" && id) void permanentlyDeleteTrashAlias(id);
      if (action === "empty-trash") void emptyTrash();
      if (action === "dismiss-message") dismissMessage();
      if (action === "toggle-favorite" && id) void toggleFavorite(id);
      if (action === "close-backup") closeBackupDialog();
      if (action === "choose-backup-file") void chooseBackupFile();
      if (action === "close-import") closeManualImport();
      if (action === "dismiss-import") void dismissShellImport();
      if (action === "toggle-suggestions") toggleSuggestions();
      if (action === "suggestion-page") showSuggestionPage(Number(button.dataset.page));
      if (action === "use-suggestion") {
        const suggestionId = button.dataset.suggestionId;
        if (suggestionId) void useSuggestion(suggestionId);
      }
      if (action === "edit" && id) openEditModal(id);
      if (action === "close-edit") closeEditModal();
      if (action === "pick-path") {
        const target = button.dataset.target;
        const kind = button.dataset.kind;
        if ((target === "create" || target === "edit") && (kind === "file" || kind === "folder")) {
          void openPathPicker(target, kind);
        }
      }
      if (action === "delete" && id) void deleteAlias(id);
    });
  });

  bindTutorialEvents();
}
