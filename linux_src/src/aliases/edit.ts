// Edit-alias modal.

import { actionLabels } from "../constants";
import { escapeHtml } from "../html";
import { clearMessages } from "../messages";
import { saveState } from "../persistence";
import { nowIso } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { AliasEntry, AliasForm } from "../types";
import { buildCommandPreview, validateAlias } from "./command";

// Opens the edit modal by copying the persisted alias into temporary editForm state.
// Changes are not saved until the modal form is submitted.
export function openEditModal(id: string) {
  const alias = state.appState.aliases.find((item) => item.id === id);
  if (!alias) return;

  state.editingId = id;
  state.editForm = {
    id: alias.id,
    name: alias.name,
    path: alias.path,
    action: alias.action,
    customCommand: alias.customCommand ?? ""
  };
  state.editError = "";
  clearMessages();
  render();
}

export function closeEditModal() {
  state.editingId = null;
  state.editForm = null;
  state.editError = "";
  render();
}

// Saves edits from the modal while preserving the original id and createdAt timestamp.
export async function updateAlias(event: SubmitEvent) {
  event.preventDefault();
  if (!state.editForm || !state.editingId) return;

  state.editError = validateAlias(state.editForm);
  if (state.editError) {
    render();
    return;
  }

  const duplicate = state.appState.aliases.find(
    (alias) => alias.name === state.editForm?.name.trim() && alias.id !== state.editingId
  );

  if (duplicate) {
    state.editError = `Alias "${state.editForm.name.trim()}" already exists.`;
    render();
    return;
  }

  const existing = state.appState.aliases.find((alias) => alias.id === state.editingId);
  if (!existing) {
    closeEditModal();
    return;
  }

  const nextAlias: AliasEntry = {
    id: existing.id,
    name: state.editForm.name.trim(),
    path: state.editForm.path.trim(),
    action: state.editForm.action,
    customCommand: state.editForm.action === "custom" ? state.editForm.customCommand.trim() : undefined,
    commandPreview: buildCommandPreview(state.editForm),
    favorite: existing.favorite,
    createdAt: existing.createdAt,
    updatedAt: nowIso()
  };

  state.appState = {
    ...state.appState,
    aliases: state.appState.aliases.map((alias) => (alias.id === existing.id ? nextAlias : alias))
  };

  state.editingId = null;
  state.editForm = null;
  state.editError = "";
  await saveState();
}

// Same as updateForm(), but scoped to the edit modal.
export function updateEditForm<K extends keyof AliasForm>(key: K, value: AliasForm[K], rerender = false) {
  if (!state.editForm) return;

  state.editForm = { ...state.editForm, [key]: value };
  state.editError = "";

  if (rerender) {
    render();
    return;
  }

  clearRenderedEditError();
  updateEditPreview();
}

export function editPreview() {
  return state.editForm ? buildCommandPreview(state.editForm) || "No command generated yet" : "";
}

export function updateEditPreview() {
  const preview = document.querySelector<HTMLElement>(".modal-preview code");
  if (preview) {
    preview.textContent = editPreview();
  }
}

export function clearRenderedEditError() {
  document.querySelector(".modal-error")?.remove();
}

// Renders the modal only when editForm/editingId are set.
// Returning an empty string keeps the main template simple.
export function renderEditModal() {
  if (!state.editForm || !state.editingId) return "";

  return `
    <section class="modal-layer" role="presentation">
      <form class="modal-card" id="edit-form" role="dialog" aria-modal="true" aria-labelledby="edit-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">Edit Alias</p>
            <h2 id="edit-title">${escapeHtml(state.editForm.name || "Alias")}</h2>
          </div>
          <button class="ghost-button modal-close" type="button" data-action="close-edit">Close</button>
        </div>

        ${state.editError ? `<p class="modal-error">${escapeHtml(state.editError)}</p>` : ""}

        <label>
          Command Name
          <input name="edit-name" value="${escapeHtml(state.editForm.name)}" placeholder="beerv2" autocomplete="off" />
        </label>

        <label>
          Location / File / Command
          <span class="path-picker-row">
            <input name="edit-path" value="${escapeHtml(state.editForm.path)}" placeholder="~/Desktop/projects/beerv2_app" autocomplete="off" />
            <button class="picker-button" type="button" title="Choose file" data-action="pick-path" data-target="edit" data-kind="file">File</button>
            <button class="picker-button" type="button" title="Choose folder" data-action="pick-path" data-target="edit" data-kind="folder">Folder</button>
          </span>
        </label>

        <label>
          Action
          <select name="edit-action">
            ${Object.entries(actionLabels)
              .map(
                ([value, label]) =>
                  `<option value="${value}" ${state.editForm?.action === value ? "selected" : ""}>${label}</option>`
              )
              .join("")}
          </select>
        </label>

        ${
          state.editForm.action === "custom"
            ? `<label>
                Custom Command
                <textarea name="edit-customCommand" rows="4" placeholder='cd "$HOME/project" && ./run.sh'>${escapeHtml(state.editForm.customCommand)}</textarea>
              </label>`
            : ""
        }

        <div class="preview modal-preview">
          <span>Preview</span>
          <code>${escapeHtml(editPreview())}</code>
        </div>

        <div class="modal-actions">
          <button class="ghost-button" type="button" data-action="close-edit">Cancel</button>
          <button class="primary-button" type="submit">Save</button>
        </div>
      </form>
    </section>
  `;
}
