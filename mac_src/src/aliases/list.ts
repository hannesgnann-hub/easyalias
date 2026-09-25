// Alias list: create, favorite, delete, search, filter, paging.

import { ChevronLeft, ChevronRight, Pencil, Star, Trash2, createIcons } from "lucide";
import { actionLabels, emptyForm } from "../constants";
import { escapeHtml } from "../html";
import { clearMessages, clearRenderedMessages } from "../messages";
import { saveBrowserTrash, saveState } from "../persistence";
import { createId, invokeCommand, isTauriRuntime, nowIso } from "../platform";
import { render } from "../render";
import { aliasPageSize, state } from "../state";
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

export function showAliasPage(page: number) {
  if (!Number.isFinite(page)) return;
  state.aliasPage = Math.max(1, Math.floor(page));
  refreshAliasResults();
}

// Save a suggestion immediately. Suggestions with an existing alias name are
// hidden in the UI, while the duplicate check also protects against stale clicks.
export async function useSuggestion(id: string) {
  const suggestion = aliasSuggestions.find((item) => item.id === id);
  if (!suggestion) return;

  if (state.appState.aliases.some((alias) => alias.name === suggestion.name)) {
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
  if (
    state.appSettings.confirmDeletes &&
    !window.confirm(`Move alias "${existing.name}" to Trash?`)
  ) {
    return;
  }

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

// Favorite changes are persisted immediately and therefore survive restarts
// as well as the existing JSON backup/export flow.
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

export function matchesAliasFilter(alias: AliasEntry) {
  const command = alias.commandPreview.trim().toLocaleLowerCase();

  switch (state.aliasFilter) {
    case "favorites":
      return Boolean(alias.favorite);
    case "git":
      return /(^|[\s;&|])git(?=\s|$)/.test(command);
    case "docker":
      return /(^|[\s;&|])docker(?:-compose)?(?=\s|$)/.test(command);
    case "navigation":
      return alias.action === "navigate";
    case "build":
      return (
        alias.action === "compile_gradle" ||
        alias.action === "compile_maven" ||
        /(^|[\s;&|])(?:\.\/)?(?:gradle|gradlew|mvn|mvnw|make)(?=\s|$)/.test(command) ||
        /(^|[\s;&|])cargo\s+build(?=\s|$)/.test(command) ||
        /(^|[\s;&|])(?:npm|pnpm|yarn|bun)(?:\s+run)?\s+build(?=\s|$)/.test(command)
      );
    case "all":
    default:
      return true;
  }
}

export function filterAliases(aliases: AliasEntry[]) {
  const query = state.aliasSearchQuery.trim().toLocaleLowerCase();

  return aliases.filter(
    (alias) =>
      matchesAliasFilter(alias) &&
      (!query ||
        alias.name.toLocaleLowerCase().includes(query) ||
        alias.commandPreview.toLocaleLowerCase().includes(query))
  );
}

export function aliasCountLabel(total: number, filtered: number) {
  const suffix = total === 1 ? "entry" : "entries";
  const hasActiveFilter = state.aliasSearchQuery.trim() || state.aliasFilter !== "all";
  return hasActiveFilter ? `${filtered} of ${total} ${suffix}` : `${total} ${suffix}`;
}

export function renderAliasResults(aliases: AliasEntry[]) {
  const filteredAliases = filterAliases(aliases);
  const aliasPageCount = Math.max(1, Math.ceil(filteredAliases.length / aliasPageSize));
  state.aliasPage = Math.min(state.aliasPage, aliasPageCount);
  const aliasPageStart = (state.aliasPage - 1) * aliasPageSize;
  const visibleAliases = filteredAliases.slice(aliasPageStart, aliasPageStart + aliasPageSize);

  if (!aliases.length) {
    return `<div class="empty-state">
      <strong>No aliases yet</strong>
      <span>Create your first command on the left.</span>
    </div>`;
  }

  if (!filteredAliases.length) {
    return `<div class="empty-state alias-search-empty">
      <strong>No matching aliases</strong>
      <span>Try another search or filter.</span>
    </div>`;
  }

  return `${visibleAliases
    .map(
      (alias) => `
        <article class="alias-row ${alias.id === state.editingId ? "selected" : ""}">
          <button
            class="favorite-button ${alias.favorite ? "active" : ""}"
            type="button"
            title="${alias.favorite ? "Remove from favorites" : "Add to favorites"}"
            aria-label="${alias.favorite ? "Remove" : "Add"} ${escapeHtml(alias.name)} ${alias.favorite ? "from" : "to"} favorites"
            aria-pressed="${Boolean(alias.favorite)}"
            data-action="toggle-favorite"
            data-id="${alias.id}"
          ><i data-lucide="star"></i></button>
          <div class="row-main">
            <span class="alias-name">${escapeHtml(alias.name)}</span>
            <span class="alias-action">${actionLabels[alias.action]}</span>
            <code>${escapeHtml(alias.commandPreview)}</code>
            <span class="created">Created ${formatDate(alias.createdAt)}</span>
          </div>
          <button class="edit-button" title="Edit alias" aria-label="Edit ${escapeHtml(alias.name)}" data-action="edit" data-id="${alias.id}"><i data-lucide="pencil"></i></button>
          <button class="icon-button" title="Move to Trash" aria-label="Move ${escapeHtml(alias.name)} to Trash" data-action="delete" data-id="${alias.id}"><i data-lucide="trash-2"></i></button>
        </article>
      `
    )
    .join("")}
    ${
      aliasPageCount > 1
        ? `<nav class="alias-pagination" aria-label="Alias pages">
            <button
              class="alias-page-button alias-page-arrow"
              type="button"
              title="Previous alias page"
              aria-label="Previous alias page"
              data-action="alias-page"
              data-page="${state.aliasPage - 1}"
              ${state.aliasPage === 1 ? "disabled" : ""}
            ><i data-lucide="chevron-left"></i></button>
            ${Array.from({ length: aliasPageCount }, (_, index) => index + 1)
              .map(
                (page) => `<button
                  class="alias-page-button${page === state.aliasPage ? " is-current" : ""}"
                  type="button"
                  aria-label="Show alias page ${page}"
                  ${page === state.aliasPage ? 'aria-current="page"' : ""}
                  data-action="alias-page"
                  data-page="${page}"
                >${page}</button>`
              )
              .join("")}
            <button
              class="alias-page-button alias-page-arrow"
              type="button"
              title="Next alias page"
              aria-label="Next alias page"
              data-action="alias-page"
              data-page="${state.aliasPage + 1}"
              ${state.aliasPage === aliasPageCount ? "disabled" : ""}
            ><i data-lucide="chevron-right"></i></button>
          </nav>`
        : ""
    }`;
}

// Search input stays mounted while only the result area is replaced. This
// avoids losing focus or jumping the caret while someone types quickly.
export function refreshAliasResults() {
  const aliases = [...state.appState.aliases].sort(compareAliases);
  const filteredAliases = filterAliases(aliases);
  const count = document.querySelector<HTMLElement>("[data-alias-count]");
  const results = document.querySelector<HTMLElement>("[data-alias-results]");

  if (count) count.textContent = aliasCountLabel(aliases.length, filteredAliases.length);
  if (!results) return;

  results.innerHTML = renderAliasResults(aliases);
  createIcons({
    icons: { ChevronLeft, ChevronRight, Pencil, Star, Trash2 },
    attrs: {
      "aria-hidden": "true",
      width: "20",
      height: "20",
      "stroke-width": "2"
    }
  });
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
