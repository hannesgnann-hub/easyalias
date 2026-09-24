// Alias trash (30-day retention).

import { actionLabels, trashRetentionSeconds } from "../constants";
import { escapeHtml } from "../html";
import { clearMessages } from "../messages";
import { saveBrowserTrash } from "../persistence";
import { invokeCommand, isTauriRuntime } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { TrashEntry, TrashMutationResult } from "../types";
import { compareAliases } from "./list";

export async function openTrash() {
  if (state.trashBusy) return;
  clearMessages();
  state.trashError = "";

  if (isTauriRuntime()) {
    try {
      state.trashEntries = await invokeCommand<TrashEntry[]>("list_trash");
    } catch (loadError) {
      state.error = `Trash could not be opened: ${String(loadError)}`;
      render();
      return;
    }
  }

  state.trashOpen = true;
  render();
}

export function closeTrash() {
  if (state.trashBusy) return;
  state.trashOpen = false;
  state.trashError = "";
  render();
}

export async function restoreTrashAlias(id: string) {
  if (state.trashBusy) return;
  const entry = state.trashEntries.find((item) => item.alias.id === id);
  if (!entry) return;
  state.trashBusy = true;
  state.trashError = "";
  render();

  try {
    if (isTauriRuntime()) {
      const result = await invokeCommand<TrashMutationResult>("restore_trash_alias", { id });
      state.appState = result.state;
      state.trashEntries = result.trash;
    } else {
      if (state.appState.aliases.some((alias) => alias.name.toLowerCase() === entry.alias.name.toLowerCase())) {
        throw new Error(`Alias "${entry.alias.name}" already exists.`);
      }
      state.appState = {
        ...state.appState,
        aliases: [...state.appState.aliases, entry.alias].sort(compareAliases)
      };
      state.trashEntries = state.trashEntries.filter((item) => item.alias.id !== id);
      localStorage.setItem("easyalias-state", JSON.stringify(state.appState));
      saveBrowserTrash();
    }
    state.notice = `Alias "${entry.alias.name}" restored.`;
  } catch (restoreError) {
    state.trashError = String(restoreError);
  } finally {
    state.trashBusy = false;
    render();
  }
}

export async function permanentlyDeleteTrashAlias(id: string) {
  if (state.trashBusy) return;
  const entry = state.trashEntries.find((item) => item.alias.id === id);
  if (!entry) return;
  if (!window.confirm(`Permanently delete alias "${entry.alias.name}"? This cannot be undone.`)) return;

  state.trashBusy = true;
  state.trashError = "";
  render();
  try {
    if (isTauriRuntime()) {
      state.trashEntries = await invokeCommand<TrashEntry[]>("permanently_delete_trash_alias", { id });
    } else {
      state.trashEntries = state.trashEntries.filter((item) => item.alias.id !== id);
      saveBrowserTrash();
    }
    state.notice = `Alias "${entry.alias.name}" permanently deleted.`;
  } catch (deleteError) {
    state.trashError = String(deleteError);
  } finally {
    state.trashBusy = false;
    render();
  }
}

export async function emptyTrash() {
  if (state.trashBusy || !state.trashEntries.length) return;
  if (!window.confirm(`Permanently delete all ${state.trashEntries.length} aliases in Trash? This cannot be undone.`)) return;

  state.trashBusy = true;
  state.trashError = "";
  render();
  try {
    if (isTauriRuntime()) {
      state.trashEntries = await invokeCommand<TrashEntry[]>("empty_trash");
    } else {
      state.trashEntries = [];
      saveBrowserTrash();
    }
    state.notice = "Trash emptied.";
  } catch (emptyError) {
    state.trashError = String(emptyError);
  } finally {
    state.trashBusy = false;
    render();
  }
}

export function formatDeletedDate(value: number) {
  return new Intl.DateTimeFormat("en-US", {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(new Date(value * 1000));
}

export function trashDaysRemaining(value: number) {
  const purgeAt = value + trashRetentionSeconds;
  return Math.max(1, Math.ceil((purgeAt - Date.now() / 1000) / (24 * 60 * 60)));
}

export function renderTrashDialog() {
  if (!state.trashOpen) return "";

  return `
    <section class="modal-layer" role="presentation">
      <section class="modal-card trash-card" role="dialog" aria-modal="true" aria-labelledby="trash-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">Recovery</p>
            <h2 id="trash-title">Trash</h2>
          </div>
          <span class="import-count">${state.trashEntries.length} deleted</span>
        </div>

        <p class="import-intro">
          Deleted aliases stay here for 30 days. Restore an alias at any time, or delete it permanently now.
        </p>

        ${state.trashError ? `<p class="modal-error">${escapeHtml(state.trashError)}</p>` : ""}

        ${
          state.trashEntries.length
            ? `<div class="trash-list" aria-label="Deleted aliases">
                ${state.trashEntries
                  .map(
                    (entry) => `
                      <article class="trash-row">
                        <div class="trash-copy">
                          <strong>${escapeHtml(entry.alias.name)}</strong>
                          <span>${escapeHtml(actionLabels[entry.alias.action])}</span>
                          <code>${escapeHtml(entry.alias.commandPreview)}</code>
                          <small>Deleted ${formatDeletedDate(entry.deletedAt)} · ${trashDaysRemaining(entry.deletedAt)} days remaining</small>
                        </div>
                        <div class="trash-row-actions">
                          <button
                            class="trash-action restore"
                            type="button"
                            title="Restore ${escapeHtml(entry.alias.name)}"
                            aria-label="Restore ${escapeHtml(entry.alias.name)}"
                            data-action="restore-trash"
                            data-id="${escapeHtml(entry.alias.id)}"
                            ${state.trashBusy ? "disabled" : ""}
                          ><i data-lucide="rotate-ccw"></i></button>
                          <button
                            class="trash-action permanent"
                            type="button"
                            title="Permanently delete ${escapeHtml(entry.alias.name)}"
                            aria-label="Permanently delete ${escapeHtml(entry.alias.name)}"
                            data-action="permanently-delete-trash"
                            data-id="${escapeHtml(entry.alias.id)}"
                            ${state.trashBusy ? "disabled" : ""}
                          ><i data-lucide="trash-2"></i></button>
                        </div>
                      </article>
                    `
                  )
                  .join("")}
              </div>`
            : `<div class="trash-empty">
                <i data-lucide="trash-2"></i>
                <strong>Trash is empty</strong>
                <span>Deleted aliases will appear here for 30 days.</span>
              </div>`
        }

        <div class="modal-actions trash-footer-actions">
          <button class="ghost-button" type="button" data-action="close-trash" ${state.trashBusy ? "disabled" : ""}>Close</button>
          <button class="danger-button" type="button" data-action="empty-trash" ${state.trashEntries.length && !state.trashBusy ? "" : "disabled"}>
            <i data-lucide="trash-2"></i>
            <span>${state.trashBusy ? "Working..." : "Empty Trash"}</span>
          </button>
        </div>
      </section>
    </section>
  `;
}
