// Loading and saving state through Tauri or browser localStorage.

import { compareAliases } from "./aliases/list";
import { trashRetentionSeconds } from "./constants";
import { clearMessages } from "./messages";
import { invokeCommand, isTauriRuntime } from "./platform";
import { render } from "./render";
import { state } from "./state";
import type { AppState, TrashEntry } from "./types";

// Loads aliases from the Rust backend in Tauri, or from localStorage in browser preview.
export async function loadState() {
  clearMessages();

  if (isTauriRuntime()) {
    try {
      state.appState = await invokeCommand<AppState>("load_aliases");
      try {
        state.trashEntries = await invokeCommand<TrashEntry[]>("list_trash");
      } catch (trashLoadError) {
        // Alias loading remains usable even if the separate trash file needs
        // attention; surface the problem without replacing native state.
        state.trashEntries = [];
        state.error = `Trash could not be loaded: ${String(trashLoadError)}`;
      }
      state.selectedImportIds = new Set(state.appState.importCandidates.map((candidate) => candidate.id));
      render();
      return;
    } catch (loadError) {
      state.error = String(loadError);
    }
  }

  const saved = localStorage.getItem("easyalias-state");
  if (saved) {
    state.appState = {
      ...state.appState,
      ...(JSON.parse(saved) as Partial<AppState>),
      importCandidates: []
    };
  }
  const savedTrash = localStorage.getItem("easyalias-trash");
  if (savedTrash) {
    const cutoff = Math.floor(Date.now() / 1000) - trashRetentionSeconds;
    state.trashEntries = (JSON.parse(savedTrash) as TrashEntry[])
      .filter((entry) => entry.deletedAt > cutoff)
      .sort((left, right) => right.deletedAt - left.deletedAt);
    localStorage.setItem("easyalias-trash", JSON.stringify(state.trashEntries));
  }

  render();
}

export function saveBrowserTrash() {
  localStorage.setItem("easyalias-trash", JSON.stringify(state.trashEntries));
}

// Persists current aliases. Tauri writes real files; browser preview only writes localStorage.
export async function saveState() {
  clearMessages();

  const aliases = [...state.appState.aliases].sort(compareAliases);

  if (isTauriRuntime()) {
    try {
      state.appState = await invokeCommand<AppState>("save_aliases", { aliases });
      state.notice = `Saved to ${state.appState.aliasTarget}`;
      render();
      return;
    } catch (saveError) {
      state.error = String(saveError);
      render();
      return;
    }
  }

  state.appState = { ...state.appState, aliases };
  localStorage.setItem("easyalias-state", JSON.stringify(state.appState));
  state.notice = "Browser preview saved. In Tauri, the app writes real files.";
  render();
}
