// Tauri/browser runtime bridge: commands, native pickers, ids.

import { updateEditForm } from "./aliases/edit";
import { updateForm } from "./aliases/list";
import { clearMessages } from "./messages";
import { render } from "./render";
import { state } from "./state";
import type { PickerKind, PickerTarget } from "./types";

// Tauri injects this marker only inside the native desktop runtime.
// Browser preview mode uses localStorage and skips native-only features.
export function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

// Small wrapper around Tauri's invoke API, keeping the rest of the code typed.
export async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

// Opens the native Linux file/folder picker through Tauri.
// In browser preview mode, there is no native dialog, so we show a friendly message.
export async function openPathPicker(target: PickerTarget, kind: PickerKind) {
  clearMessages();
  state.editError = "";

  if (!isTauriRuntime()) {
    state.error = "The file/folder picker only works in the Tauri app, not in browser preview.";
    render();
    return;
  }

  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: false,
      directory: kind === "folder"
    });

    if (typeof selected !== "string") return;

    if (target === "create") {
      updateForm("path", selected);
      const input = document.querySelector<HTMLInputElement>('input[name="path"]');
      if (input) input.value = selected;
      return;
    }

    if (target === "edit") {
      updateEditForm("path", selected);
      const input = document.querySelector<HTMLInputElement>('input[name="edit-path"]');
      if (input) input.value = selected;
      return;
    }

    if (state.automationEditor) {
      state.automationEditor = { ...state.automationEditor, path: selected };
      const input = document.querySelector<HTMLInputElement>('input[name="automation-path"]');
      if (input) input.value = selected;
    }
  } catch (pickerError) {
    const message = `Picker could not be opened: ${String(pickerError)}`;
    if (target === "automation") {
      state.automationError = message;
    } else if (target === "edit") {
      state.editError = message;
    } else {
      state.error = message;
    }
    render();
  }
}

// Static footer links share the opener plugin so every external link opens in
// the user's default browser instead of inside the Tauri WebView.
export async function openExternalLink(event: Event) {
  event.preventDefault();
  const anchor = event.currentTarget as HTMLAnchorElement;
  const targetUrl = anchor.href;

  if (!isTauriRuntime()) {
    window.open(targetUrl, "_blank", "noopener,noreferrer");
    return;
  }

  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(targetUrl);
  } catch (openError) {
    state.error = `Link could not be opened: ${String(openError)}`;
    render();
  }
}

// Prefer a browser UUID. The fallback only exists for older WebViews.
export function createId() {
  if ("crypto" in window && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }

  return `alias_${Date.now()}_${Math.random().toString(16).slice(2)}`;
}

// Store timestamps as ISO strings because they are easy to persist and format later.
export function nowIso() {
  return new Date().toISOString();
}
