// Loading and saving state through Tauri or browser localStorage.

import { compareAliases } from "./aliases/list";
import { SUN_REGION_OPTIONS, trashRetentionSeconds } from "./constants";
import { clearMessages } from "./messages";
import { invokeCommand, isTauriRuntime } from "./platform";
import { applyTheme, persistStoredSettings, readStoredSettings } from "./preferences";
import { render } from "./render";
import { state } from "./state";
import type {
  AppSettings,
  AppState,
  Automation,
  SunLocationSetting,
  SunRegionOption,
  TimedAutomation,
  TrashEntry
} from "./types";

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
      try {
        state.automations = await invokeCommand<Automation[]>("load_automations");
      } catch (automationLoadError) {
        state.automations = [];
        state.error = `Automations could not be loaded: ${String(automationLoadError)}`;
      }
      try {
        state.timedAutomations = await invokeCommand<TimedAutomation[]>("list_timed_automations");
      } catch (timedAutomationLoadError) {
        state.timedAutomations = [];
        state.error = `Timed automations could not be loaded: ${String(timedAutomationLoadError)}`;
      }
      try {
        state.sunRegionOptions = await invokeCommand<SunRegionOption[]>("list_sun_regions");
        state.sunLocation = await invokeCommand<SunLocationSetting>("load_sun_location_state");
      } catch (sunLocationLoadError) {
        state.error = `Sunrise/sunset region could not be loaded: ${String(sunLocationLoadError)}`;
      }
      try {
        state.appSettings = await invokeCommand<AppSettings>("load_settings");
        persistStoredSettings(state.appSettings);
        applyTheme(state.appSettings.theme);
      } catch (settingsLoadError) {
        state.error = `Settings could not be loaded: ${String(settingsLoadError)}`;
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
    // Merge instead of replacing so older browser-preview state from the earlier
    // PowerShell version does not drop the newer commandDir/path fields.
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
  const savedAutomations = localStorage.getItem("easyalias-automations");
  if (savedAutomations) {
    state.automations = (JSON.parse(savedAutomations) as Automation[]).map((automation) => ({
      ...automation,
      group: automation.group ?? ""
    }));
  }
  const savedTimedAutomations = localStorage.getItem("easyalias-timed-automations");
  if (savedTimedAutomations) {
    state.timedAutomations = JSON.parse(savedTimedAutomations) as TimedAutomation[];
  }
  state.sunRegionOptions = SUN_REGION_OPTIONS;
  const savedSunLocation = localStorage.getItem("easyalias-sun-location");
  state.sunLocation = savedSunLocation ? (JSON.parse(savedSunLocation) as SunLocationSetting) : { region: "eu-central" };

  state.appSettings = readStoredSettings();
  applyTheme(state.appSettings.theme);

  render();
}

export function saveBrowserSunLocation() {
  localStorage.setItem("easyalias-sun-location", JSON.stringify(state.sunLocation));
}

export function saveBrowserTimedAutomations() {
  localStorage.setItem("easyalias-timed-automations", JSON.stringify(state.timedAutomations));
}

export function saveBrowserTrash() {
  localStorage.setItem("easyalias-trash", JSON.stringify(state.trashEntries));
}

export function saveBrowserAutomationTrash() {
  localStorage.setItem("easyalias-automation-trash", JSON.stringify(state.automationTrashEntries));
}

// Persists current aliases. Tauri writes real files; browser preview only writes localStorage.
export async function saveState() {
  clearMessages();

  const aliases = [...state.appState.aliases].sort(compareAliases);

  if (isTauriRuntime()) {
    try {
      // The backend is authoritative for files and PATH status. It may also
      // normalize old commandPreview values into the current cmd.exe format.
      state.appState = await invokeCommand<AppState>("save_aliases", { aliases });
      state.notice = `Saved: ${state.appState.commandDir}`;
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
