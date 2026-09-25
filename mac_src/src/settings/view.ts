// Settings view.

import { ArrowLeft, Monitor, Moon, Sun, X, createIcons } from "lucide";
import { repoUrl, sponsorUrl } from "../constants";
import { replaceAppHtml } from "../a11y";
import { appElement } from "../dom";
import { escapeHtml } from "../html";
import { closeSettingsView } from "../navigation";
import { invokeCommand, isTauriRuntime, openExternalLink } from "../platform";
import { applyAccessibilityPreferences, applyTheme, persistStoredSettings } from "../preferences";
import { render } from "../render";
import { state } from "../state";
import type { AccessibilitySetting, AppSettings, HotkeyBehavior, ThemePreference } from "../types";

// Applies a theme choice: immediate visual switch, optimistic local persistence,
// then the backend write (which is the source of truth in the native app).
export async function updateThemePreference(theme: ThemePreference) {
  if (state.appSettings.theme === theme) return;
  state.appSettings = { ...state.appSettings, theme };
  applyTheme(theme);
  persistStoredSettings(state.appSettings);
  render();
  await saveSettingsToBackend();
}

export async function updateHotkeyBehavior(behavior: HotkeyBehavior) {
  if (state.appSettings.hotkeyBehavior === behavior) return;
  state.appSettings = { ...state.appSettings, hotkeyBehavior: behavior };
  persistStoredSettings(state.appSettings);
  render();
  await saveSettingsToBackend();
}

export async function updateShowSuggestions(show: boolean) {
  if (state.appSettings.showSuggestions === show) return;
  state.appSettings = { ...state.appSettings, showSuggestions: show };
  persistStoredSettings(state.appSettings);
  render();
  await saveSettingsToBackend();
}

export async function updateAutostart(enabled: boolean) {
  if (state.appSettings.autostart === enabled) return;
  state.appSettings = { ...state.appSettings, autostart: enabled };
  persistStoredSettings(state.appSettings);
  render();
  await saveSettingsToBackend();
}

export async function updateAccessibilitySetting(setting: AccessibilitySetting, enabled: boolean) {
  if (state.appSettings[setting] === enabled) return;
  state.appSettings = { ...state.appSettings, [setting]: enabled };
  applyAccessibilityPreferences(state.appSettings);
  persistStoredSettings(state.appSettings);
  render();
  await saveSettingsToBackend();
}

const accessibilityOptions: { setting: AccessibilitySetting; title: string; description: string }[] = [
  {
    setting: "keepMessages",
    title: "Keep messages until dismissed",
    description: "Status and error messages stay on screen until you close them, instead of disappearing after a few seconds."
  },
  {
    setting: "largeUi",
    title: "Larger text and controls",
    description: "Enlarges text, buttons and click targets throughout EasyAlias."
  },
  {
    setting: "reduceMotion",
    title: "Reduce motion",
    description: "Turns off animations and transitions. EasyAlias already follows the macOS \"Reduce motion\" setting; this forces it on."
  },
  {
    setting: "confirmDeletes",
    title: "Confirm before deleting aliases",
    description: "Asks before an alias is moved to the Trash, so a slip of the hand does not remove it."
  }
];

function renderAccessibilityCard() {
  return `
      <div class="settings-card">
        <div class="settings-card-head">
          <h2>Accessibility</h2>
          <p>EasyAlias works with VoiceOver and the keyboard alone: Tab moves between controls and Escape closes dialogs. These options adapt it further.</p>
        </div>
        <div class="settings-toggle-list">
          ${accessibilityOptions
            .map((option) => {
              const enabled = state.appSettings[option.setting];
              const labelId = `a11y-${option.setting}-label`;
              const descriptionId = `a11y-${option.setting}-description`;
              return `
                <div class="settings-toggle-row">
                  <div class="settings-toggle-copy">
                    <span class="settings-toggle-title" id="${labelId}">${option.title}</span>
                    <span class="settings-segment-option-hint" id="${descriptionId}">${option.description}</span>
                  </div>
                  <div class="settings-segment" role="group" aria-labelledby="${labelId}" aria-describedby="${descriptionId}">
                    <button type="button" class="settings-segment-option ${enabled ? "is-selected" : ""}" aria-pressed="${enabled}" data-settings-action="set-accessibility" data-setting="${option.setting}" data-value="on" ${state.settingsBusy ? "disabled" : ""}><span>On</span></button>
                    <button type="button" class="settings-segment-option ${!enabled ? "is-selected" : ""}" aria-pressed="${!enabled}" data-settings-action="set-accessibility" data-setting="${option.setting}" data-value="off" ${state.settingsBusy ? "disabled" : ""}><span>Off</span></button>
                  </div>
                </div>`;
            })
            .join("")}
        </div>
      </div>`;
}

export async function saveSettingsToBackend() {
  if (!isTauriRuntime()) return;
  state.settingsBusy = true;
  try {
    state.appSettings = await invokeCommand<AppSettings>("save_settings", { settings: state.appSettings });
    persistStoredSettings(state.appSettings);
    state.settingsError = "";
  } catch (saveError) {
    state.settingsError = `Settings could not be saved: ${String(saveError)}`;
  } finally {
    state.settingsBusy = false;
    render();
  }
}

export function renderSettingsView() {
  const themeOptions: { value: ThemePreference; label: string; icon: string }[] = [
    { value: "light", label: "Light", icon: "sun" },
    { value: "dark", label: "Dark", icon: "moon" },
    { value: "system", label: "System", icon: "monitor" }
  ];
  const behaviorOptions: { value: HotkeyBehavior; label: string; hint: string }[] = [
    { value: "window", label: "Show run window", hint: "Brings EasyAlias forward and shows live output." },
    { value: "background", label: "Run in background", hint: "Runs silently with only a short status message." }
  ];

  replaceAppHtml(`
    <section class="shell settings-shell">
      <header class="topbar">
        <div>
          <p class="eyebrow">EasyAlias</p>
          <h1>Settings</h1>
        </div>
        <div class="topbar-actions">
          <button class="header-icon-button" type="button" title="Back" aria-label="Back" data-settings-action="back"><i data-lucide="arrow-left"></i></button>
        </div>
      </header>

      ${
        state.settingsError
          ? `<div class="message-banner error"><span data-announce="assertive">${escapeHtml(state.settingsError)}</span><button class="message-dismiss" type="button" title="Dismiss message" aria-label="Dismiss message" data-settings-action="dismiss-message"><i data-lucide="x"></i></button></div>`
          : ""
      }

      <div class="settings-card">
        <div class="settings-card-head">
          <h2>Appearance</h2>
          <p>Choose how EasyAlias looks. "System" follows your macOS light/dark setting.</p>
        </div>
        <div class="settings-segment" role="group" aria-label="Theme">
          ${themeOptions
            .map(
              (option) => `
                <button
                  type="button"
                  class="settings-segment-option ${state.appSettings.theme === option.value ? "is-selected" : ""}"
                  aria-pressed="${state.appSettings.theme === option.value}"
                  data-settings-action="set-theme"
                  data-value="${option.value}"
                  ${state.settingsBusy ? "disabled" : ""}
                ><i data-lucide="${option.icon}"></i><span>${option.label}</span></button>`
            )
            .join("")}
        </div>
      </div>

      <div class="settings-card">
        <div class="settings-card-head">
          <h2>Automation shortcuts</h2>
          <p>What happens when you press an automation's global keyboard shortcut. Closing the window keeps EasyAlias running in the menu bar so shortcuts stay active &mdash; use the menu-bar icon to quit.</p>
        </div>
        <div class="settings-segment settings-segment-stacked" role="group" aria-label="Shortcut behavior">
          ${behaviorOptions
            .map(
              (option) => `
                <button
                  type="button"
                  class="settings-segment-option settings-segment-option-wide ${state.appSettings.hotkeyBehavior === option.value ? "is-selected" : ""}"
                  aria-pressed="${state.appSettings.hotkeyBehavior === option.value}"
                  data-settings-action="set-hotkey-behavior"
                  data-value="${option.value}"
                  ${state.settingsBusy ? "disabled" : ""}
                >
                  <span class="settings-segment-option-label">${option.label}</span>
                  <span class="settings-segment-option-hint">${option.hint}</span>
                </button>`
            )
            .join("")}
        </div>
        <p class="settings-hint">Assign a shortcut to an automation from the keyboard button on its card.</p>
      </div>

      <div class="settings-card">
        <div class="settings-card-head">
          <h2>Start at login</h2>
          <p>Launch EasyAlias automatically (hidden in the menu bar) when you log in, so automation shortcuts are ready right away.</p>
        </div>
        <div class="settings-segment" role="group" aria-label="Start at login">
          <button type="button" class="settings-segment-option ${state.appSettings.autostart ? "is-selected" : ""}" aria-pressed="${state.appSettings.autostart}" data-settings-action="set-autostart" data-value="on" ${state.settingsBusy ? "disabled" : ""}><span>On</span></button>
          <button type="button" class="settings-segment-option ${!state.appSettings.autostart ? "is-selected" : ""}" aria-pressed="${!state.appSettings.autostart}" data-settings-action="set-autostart" data-value="off" ${state.settingsBusy ? "disabled" : ""}><span>Off</span></button>
        </div>
      </div>

      <div class="settings-card">
        <div class="settings-card-head">
          <h2>Alias suggestions</h2>
          <p>Show the built-in list of suggested aliases above your own aliases in the alias view.</p>
        </div>
        <div class="settings-segment" role="group" aria-label="Alias suggestions">
          <button type="button" class="settings-segment-option ${state.appSettings.showSuggestions ? "is-selected" : ""}" aria-pressed="${state.appSettings.showSuggestions}" data-settings-action="set-suggestions" data-value="on" ${state.settingsBusy ? "disabled" : ""}><span>On</span></button>
          <button type="button" class="settings-segment-option ${!state.appSettings.showSuggestions ? "is-selected" : ""}" aria-pressed="${!state.appSettings.showSuggestions}" data-settings-action="set-suggestions" data-value="off" ${state.settingsBusy ? "disabled" : ""}><span>Off</span></button>
        </div>
      </div>

      ${renderAccessibilityCard()}

      <aside class="support-banner" aria-label="Support EasyAlias"><span>Support EasyAlias development</span><a href="${sponsorUrl}" target="_blank" rel="noreferrer" data-external-link>❤ Become a sponsor</a><a class="support-star" href="${repoUrl}" target="_blank" rel="noreferrer" data-external-link>★ Give us a star on GitHub</a></aside>
    </section>
  `);

  createIcons({
    icons: { ArrowLeft, Monitor, Moon, Sun, X },
    attrs: { "aria-hidden": "true", width: "20", height: "20", "stroke-width": "2" }
  });

  document.querySelectorAll<HTMLAnchorElement>("[data-external-link]").forEach((link) => {
    link.addEventListener("click", openExternalLink);
  });

  appElement.querySelector<HTMLElement>(".settings-shell")?.addEventListener("click", (event) => {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-settings-action]");
    if (!button) return;
    const action = button.dataset.settingsAction;
    const value = button.dataset.value;
    if (action === "back") closeSettingsView();
    else if (action === "dismiss-message") {
      state.settingsError = "";
      render();
    } else if (action === "set-theme" && value) {
      void updateThemePreference(value as ThemePreference);
    } else if (action === "set-hotkey-behavior" && value) {
      void updateHotkeyBehavior(value as HotkeyBehavior);
    } else if (action === "set-suggestions" && value) {
      void updateShowSuggestions(value === "on");
    } else if (action === "set-autostart" && value) {
      void updateAutostart(value === "on");
    } else if (action === "set-accessibility" && value && button.dataset.setting) {
      void updateAccessibilitySetting(button.dataset.setting as AccessibilitySetting, value === "on");
    }
  });
}
