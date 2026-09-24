// Automation trash.

import { formatDeletedDate, trashDaysRemaining } from "../aliases/trash";
import { trashRetentionSeconds } from "../constants";
import { escapeHtml } from "../html";
import { clearMessages } from "../messages";
import { saveBrowserAutomationTrash } from "../persistence";
import { invokeCommand, isTauriRuntime } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { AutomationTrashEntry, AutomationTrashMutationResult } from "../types";
import { persistAutomations } from "./editor";
import { compareAutomations } from "./filters";

export async function openAutomationTrash() {
  clearMessages();
  state.automationTrashError = "";
  state.automationTrashOpen = true;

  if (isTauriRuntime()) {
    state.automationTrashBusy = true;
    render();
    try {
      state.automationTrashEntries = await invokeCommand<AutomationTrashEntry[]>("list_automation_trash");
    } catch (trashLoadError) {
      state.automationTrashError = String(trashLoadError);
    } finally {
      state.automationTrashBusy = false;
    }
  } else {
    const cutoff = Math.floor(Date.now() / 1000) - trashRetentionSeconds;
    state.automationTrashEntries = state.automationTrashEntries
      .filter((entry) => entry.deletedAt > cutoff)
      .sort((left, right) => right.deletedAt - left.deletedAt);
    saveBrowserAutomationTrash();
  }

  render();
}

export function closeAutomationTrash() {
  if (state.automationTrashBusy) return;
  state.automationTrashOpen = false;
  state.automationTrashError = "";
  render();
}

export async function restoreTrashAutomation(id: string) {
  if (state.automationTrashBusy) return;
  const entry = state.automationTrashEntries.find((item) => item.automation.id === id);
  if (!entry) return;

  state.automationTrashBusy = true;
  state.automationTrashError = "";
  render();
  try {
    if (isTauriRuntime()) {
      const result = await invokeCommand<AutomationTrashMutationResult>("restore_trash_automation", { id });
      state.automations = result.automations;
      state.automationTrashEntries = result.trash;
    } else {
      const duplicate = state.automations.find(
        (automation) =>
          automation.id === entry.automation.id ||
          automation.name.trim().toLocaleLowerCase() === entry.automation.name.trim().toLocaleLowerCase()
      );
      if (duplicate) {
        throw new Error(`Automation "${entry.automation.name}" already exists.`);
      }
      await persistAutomations([...state.automations, entry.automation].sort(compareAutomations));
      state.automationTrashEntries = state.automationTrashEntries.filter((item) => item.automation.id !== id);
      saveBrowserAutomationTrash();
    }
    state.notice = `Automation "${entry.automation.name}" restored.`;
  } catch (restoreError) {
    state.automationTrashError = String(restoreError);
  } finally {
    state.automationTrashBusy = false;
    render();
  }
}

export async function permanentlyDeleteTrashAutomation(id: string) {
  if (state.automationTrashBusy) return;
  const entry = state.automationTrashEntries.find((item) => item.automation.id === id);
  if (!entry || !window.confirm(`Permanently delete automation "${entry.automation.name}"? This cannot be undone.`)) {
    return;
  }

  state.automationTrashBusy = true;
  state.automationTrashError = "";
  render();
  try {
    if (isTauriRuntime()) {
      state.automationTrashEntries = await invokeCommand<AutomationTrashEntry[]>(
        "permanently_delete_trash_automation",
        { id }
      );
    } else {
      state.automationTrashEntries = state.automationTrashEntries.filter((item) => item.automation.id !== id);
      saveBrowserAutomationTrash();
    }
  } catch (deleteError) {
    state.automationTrashError = String(deleteError);
  } finally {
    state.automationTrashBusy = false;
    render();
  }
}

export async function emptyAutomationTrash() {
  if (state.automationTrashBusy || state.automationTrashEntries.length === 0) return;
  const count = state.automationTrashEntries.length;
  if (!window.confirm(`Permanently delete all ${count} automation${count === 1 ? "" : "s"} in Trash? This cannot be undone.`)) {
    return;
  }

  state.automationTrashBusy = true;
  state.automationTrashError = "";
  render();
  try {
    if (isTauriRuntime()) {
      state.automationTrashEntries = await invokeCommand<AutomationTrashEntry[]>("empty_automation_trash");
    } else {
      state.automationTrashEntries = [];
      saveBrowserAutomationTrash();
    }
  } catch (emptyError) {
    state.automationTrashError = String(emptyError);
  } finally {
    state.automationTrashBusy = false;
    render();
  }
}

export function renderAutomationTrashDialog() {
  if (!state.automationTrashOpen) return "";

  return `
    <section class="modal-layer" role="presentation">
      <section class="modal-card trash-card" role="dialog" aria-modal="true" aria-labelledby="automation-trash-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">Recovery</p>
            <h2 id="automation-trash-title">Automation Trash</h2>
          </div>
          <span class="import-count">${state.automationTrashEntries.length} deleted</span>
        </div>

        <p class="import-intro">
          Deleted automations stay here for 30 days. Restore a workflow at any time, or delete it permanently now.
        </p>

        ${state.automationTrashError ? `<p class="modal-error">${escapeHtml(state.automationTrashError)}</p>` : ""}

        ${
          state.automationTrashEntries.length
            ? `<div class="trash-list" aria-label="Deleted automations">
                ${state.automationTrashEntries
                  .map(
                    (entry) => `
                      <article class="trash-row">
                        <div class="trash-copy">
                          <strong>${escapeHtml(entry.automation.name)}</strong>
                          <span>${entry.automation.steps.length} ${entry.automation.steps.length === 1 ? "step" : "steps"}</span>
                          <code>${escapeHtml(entry.automation.path)}</code>
                          <small>Deleted ${formatDeletedDate(entry.deletedAt)} · ${trashDaysRemaining(entry.deletedAt)} days remaining</small>
                        </div>
                        <div class="trash-row-actions">
                          <button
                            class="trash-action restore"
                            type="button"
                            title="Restore ${escapeHtml(entry.automation.name)}"
                            aria-label="Restore ${escapeHtml(entry.automation.name)}"
                            data-automation-action="restore-trash"
                            data-id="${escapeHtml(entry.automation.id)}"
                            ${state.automationTrashBusy ? "disabled" : ""}
                          ><i data-lucide="rotate-ccw"></i></button>
                          <button
                            class="trash-action permanent"
                            type="button"
                            title="Permanently delete ${escapeHtml(entry.automation.name)}"
                            aria-label="Permanently delete ${escapeHtml(entry.automation.name)}"
                            data-automation-action="delete-trash-permanently"
                            data-id="${escapeHtml(entry.automation.id)}"
                            ${state.automationTrashBusy ? "disabled" : ""}
                          ><i data-lucide="trash-2"></i></button>
                        </div>
                      </article>`
                  )
                  .join("")}
              </div>`
            : `<div class="trash-empty"><strong>Automation Trash is empty</strong><span>Deleted workflows will stay recoverable here for 30 days.</span></div>`
        }

        <div class="modal-actions trash-footer-actions">
          <button class="ghost-button" type="button" data-automation-action="close-trash" ${state.automationTrashBusy ? "disabled" : ""}>Close</button>
          <button class="danger-button" type="button" data-automation-action="empty-trash" ${state.automationTrashEntries.length && !state.automationTrashBusy ? "" : "disabled"}>
            <i data-lucide="trash-2"></i><span>${state.automationTrashBusy ? "Working..." : "Empty Trash"}</span>
          </button>
        </div>
      </section>
    </section>`;
}
