// Alias list: create, favorite, delete, search, filter, paging.

import { emptyForm } from "../constants";
import { clearMessages, clearRenderedMessages } from "../messages";
import { saveBrowserTrash, saveState } from "../persistence";
import { createId, invokeCommand, isTauriRuntime, nowIso } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { AliasEntry, AliasForm, TrashMutationResult } from "../types";
import { buildCommandPreview, validateAlias } from "./command";
import { aliasSuggestions } from "./suggestions";

export function toggleSuggestions() {
  state.suggestionsExpanded = !state.suggestionsExpanded;
  render();
}

// Page numbers come from HTML data attributes, so normalize them before the
// next render applies the upper bound for the currently available suggestions.
export function showSuggestionPage(page: number) {
  if (!Number.isFinite(page)) return;
  state.suggestionPage = Math.max(1, Math.floor(page));
  render();
}

// Save a suggestion immediately. Suggestions with an existing alias name are
// hidden in the UI, while the duplicate check also protects against stale clicks.
export async function useSuggestion(id: string) {
  const suggestion = aliasSuggestions.find((item) => item.id === id);
  if (!suggestion) return;

  if (state.appState.aliases.some((alias) => alias.name.toLowerCase() === suggestion.name.toLowerCase())) {
    state.error = `Alias "${suggestion.name}" already exists.`;
    render();
    return;
  }

  const timestamp = nowIso();
  const nextAlias: AliasEntry = {
    id: createId(),
    name: suggestion.name,
    path: suggestion.path,
    action: suggestion.action,
    customCommand: suggestion.action === "custom" ? suggestion.customCommand : undefined,
    commandPreview: buildCommandPreview(suggestion),
    favorite: false,
    createdAt: timestamp,
    updatedAt: timestamp
  };

  state.appState = {
    ...state.appState,
    aliases: [...state.appState.aliases, nextAlias]
  };
  clearMessages();
  await saveState();
}

export async function upsertAlias(event: SubmitEvent) {
  event.preventDefault();
  clearMessages();

  const validationError = validateAlias(state.form);
  if (validationError) {
    state.error = validationError;
    render();
    return;
  }

  const duplicate = state.appState.aliases.find(
    (alias) => alias.name === state.form.name.trim()
  );

  if (duplicate) {
    state.error = `Alias "${state.form.name.trim()}" already exists.`;
    render();
    return;
  }

  const timestamp = nowIso();
  const nextAlias: AliasEntry = {
    id: createId(),
    name: state.form.name.trim(),
    path: state.form.path.trim(),
    action: state.form.action,
    customCommand: state.form.action === "custom" ? state.form.customCommand.trim() : undefined,
    commandPreview: buildCommandPreview(state.form),
    favorite: false,
    createdAt: timestamp,
    updatedAt: timestamp
  };

  state.appState = {
    ...state.appState,
    aliases: [...state.appState.aliases, nextAlias]
  };

  state.form = { ...emptyForm };
  await saveState();
}

// Deleting now moves the alias into the recoverable 30-day trash instead of
// removing it immediately from disk.
export async function deleteAlias(id: string) {
  const existing = state.appState.aliases.find((alias) => alias.id === id);
  if (!existing) return;

  clearMessages();

  if (isTauriRuntime()) {
    try {
      const result = await invokeCommand<TrashMutationResult>("move_alias_to_trash", { id });
      state.appState = result.state;
      state.trashEntries = result.trash;
    } catch (deleteError) {
      state.error = String(deleteError);
      render();
      return;
    }
  } else {
    state.appState = {
      ...state.appState,
      aliases: state.appState.aliases.filter((alias) => alias.id !== id)
    };
    state.trashEntries = [
      { alias: existing, deletedAt: Math.floor(Date.now() / 1000) },
      ...state.trashEntries.filter((entry) => entry.alias.id !== id)
    ];
    localStorage.setItem("easyalias-state", JSON.stringify(state.appState));
    saveBrowserTrash();
  }

  if (state.editingId === id) {
    state.editingId = null;
    state.editForm = null;
    state.editError = "";
  }

  state.notice = `Alias "${existing.name}" moved to Trash.`;
  render();
}

export async function toggleFavorite(id: string) {
  const existing = state.appState.aliases.find((alias) => alias.id === id);
  if (!existing) return;

  state.appState = {
    ...state.appState,
    aliases: state.appState.aliases.map((alias) =>
      alias.id === id
        ? { ...alias, favorite: !Boolean(alias.favorite), updatedAt: nowIso() }
        : alias
    )
  };
  await saveState();
}

// Updates the create form. Most text changes update only the command preview,
// avoiding a full re-render so input focus is not lost while typing.
export function updateForm<K extends keyof AliasForm>(key: K, value: AliasForm[K], rerender = false) {
  state.form = { ...state.form, [key]: value };
  clearMessages();

  if (rerender) {
    render();
    return;
  }

  clearRenderedMessages();
  updatePreview();
}

// Centralized display formatting for timestamps shown in alias cards.
export function formatDate(value: string) {
  return new Intl.DateTimeFormat("en-US", {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(new Date(value));
}

// Favorites are grouped first; each group remains predictable and alphabetical.
export function compareAliases(left: AliasEntry, right: AliasEntry) {
  const favoriteDifference = Number(Boolean(right.favorite)) - Number(Boolean(left.favorite));
  return favoriteDifference || left.name.localeCompare(right.name);
}

export function formPreview() {
  return buildCommandPreview(state.form) || "No command generated yet";
}

export function updatePreview() {
  const preview = document.querySelector<HTMLElement>(".preview code");
  if (preview) {
    preview.textContent = formPreview();
  }
}
