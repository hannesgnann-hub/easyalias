// Theme, locally stored settings and hotkey accelerator formatting.

import { settingsStorageKey } from "./constants";
import type { AppSettings, ThemePreference } from "./types";

// macOS-style glyphs for a Tauri accelerator string, e.g.
// "CmdOrCtrl+Shift+L" -> "⌘⇧L". Other platforms spell the modifiers out.
export function formatAccelerator(accelerator: string): string {
  const isMac = navigator.platform.toUpperCase().includes("MAC");
  return accelerator
    .split("+")
    .map((token) => {
      const key = token.trim();
      const lower = key.toLowerCase();
      if (lower === "cmdorctrl" || lower === "commandorcontrol") return isMac ? "⌘" : "Ctrl+";
      if (lower === "cmd" || lower === "command" || lower === "super" || lower === "meta")
        return isMac ? "⌘" : "Super+";
      if (lower === "ctrl" || lower === "control") return isMac ? "⌃" : "Ctrl+";
      if (lower === "alt" || lower === "option") return isMac ? "⌥" : "Alt+";
      if (lower === "shift") return isMac ? "⇧" : "Shift+";
      if (key.length === 1) return key.toUpperCase();
      return key;
    })
    .join("")
    .replace(/\+$/, "");
}

// Builds a Tauri accelerator string from a keydown event, or null if the
// press is not a usable shortcut (no modifier, or a lone modifier key).
export function acceleratorFromEvent(event: KeyboardEvent): string | null {
  const parts: string[] = [];
  if (event.metaKey) parts.push("CmdOrCtrl");
  if (event.ctrlKey && !event.metaKey) parts.push("CmdOrCtrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");

  const key = event.key;
  if (["Meta", "Control", "Alt", "Shift", "OS", "Dead"].includes(key)) return null;
  if (parts.length === 0) return null;

  let token: string;
  if (key === " " || key === "Spacebar") token = "Space";
  else if (key.length === 1) token = key.toUpperCase();
  else token = key;

  // De-duplicate (Ctrl on mac maps to the same slot as Cmd).
  const modifiers = Array.from(new Set(parts));
  return [...modifiers, token].join("+");
}

// Resolves a theme preference to the concrete light/dark the page should show
// and stamps it on <html> so styles.css can switch tokens.
export function applyTheme(theme: ThemePreference) {
  const prefersDark =
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches;
  const dark = theme === "dark" || (theme === "system" && prefersDark);
  document.documentElement.setAttribute("data-theme", dark ? "dark" : "light");
}

// Stamps the accessibility preferences on <html> so styles.css can enlarge the
// interface or switch off motion. The OS "reduce motion" setting is honored in
// CSS on its own; this attribute forces it on regardless.
export function applyAccessibilityPreferences(settings: AppSettings) {
  const root = document.documentElement;
  root.toggleAttribute("data-large-ui", settings.largeUi);
  root.toggleAttribute("data-reduce-motion", settings.reduceMotion);
}

export function readStoredSettings(): AppSettings {
  try {
    const raw = localStorage.getItem(settingsStorageKey);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<AppSettings>;
      return {
        theme:
          parsed.theme === "light" || parsed.theme === "dark" || parsed.theme === "system"
            ? parsed.theme
            : "system",
        hotkeyBehavior: parsed.hotkeyBehavior === "background" ? "background" : "window",
        showSuggestions: parsed.showSuggestions !== false,
        autostart: parsed.autostart === true,
        keepMessages: parsed.keepMessages === true,
        largeUi: parsed.largeUi === true,
        reduceMotion: parsed.reduceMotion === true,
        confirmDeletes: parsed.confirmDeletes === true
      };
    }
  } catch {
    // Ignore unreadable storage - fall back to defaults.
  }
  return {
    theme: "system",
    hotkeyBehavior: "window",
    showSuggestions: true,
    autostart: false,
    keepMessages: false,
    largeUi: false,
    reduceMotion: false,
    confirmDeletes: false
  };
}

export function persistStoredSettings(settings: AppSettings) {
  try {
    localStorage.setItem(settingsStorageKey, JSON.stringify(settings));
  } catch {
    // Non-fatal - the backend copy is the source of truth in the native app.
  }
}
