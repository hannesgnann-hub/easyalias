// Switching between the alias, automation and settings views.

import { clearMessages } from "./messages";
import { render } from "./render";
import { state } from "./state";

// Collapses every automation card's inline popover (group / schedule /
// hotkey) so only one interaction surface is ever open at a time.
export function closeAllAutomationCardPopovers() {
  state.automationGroupPickerId = null;
  state.scheduleEditorAutomationId = null;
  state.hotkeyEditorAutomationId = null;
  state.hotkeyDraft = null;
  state.hotkeyCaptureError = "";
}

export function openAutomationsView() {
  clearMessages();
  state.automationError = "";
  closeAllAutomationCardPopovers();
  state.timedAutomationEditor = null;
  state.currentView = "automations";
  render();
}

export function openSettingsView() {
  clearMessages();
  state.settingsError = "";
  state.settingsReturnView = state.currentView === "automations" ? "automations" : "aliases";
  state.currentView = "settings";
  render();
}

export function closeSettingsView() {
  state.settingsError = "";
  state.currentView = state.settingsReturnView;
  render();
}

export function closeAutomationsView() {
  if (state.automationRun?.running) return;
  state.automationEditor = null;
  state.automationTrashOpen = false;
  state.automationTrashError = "";
  state.automationError = "";
  state.automationRun = null;
  closeAllAutomationCardPopovers();
  state.timedAutomationEditor = null;
  state.currentView = "aliases";
  render();
}
