// Top-level render of the alias view and shared modals.

import {
  ChevronLeft,
  ChevronRight,
  FileDown,
  FileUp,
  RotateCcw,
  SquareTerminal,
  Star,
  Trash2,
  X,
  createIcons
} from "lucide";
import { renderBackupDialog } from "./aliases/backup";
import { buildCommandPreview } from "./aliases/command";
import { renderEditModal } from "./aliases/edit";
import { compareAliases, formPreview, formatDate } from "./aliases/list";
import { renderImportModal } from "./aliases/shellImport";
import { aliasSuggestions } from "./aliases/suggestions";
import { renderTrashDialog } from "./aliases/trash";
import { actionLabels, redditUrl, repoUrl, websiteUrl } from "./constants";
import { appElement } from "./dom";
import { bindEvents } from "./events";
import { escapeHtml } from "./html";
import { scheduleMessageDismissal } from "./messages";
import { isTauriRuntime } from "./platform";
import { state, suggestionPageSize } from "./state";

// Main render function. This replaces the app HTML from state and then calls bindEvents().
// For a larger app, this would be a good candidate to split into smaller render helpers.
export function render() {
  const aliases = [...state.appState.aliases].sort(compareAliases);
  const existingNames = new Set(aliases.map((alias) => alias.name));
  const availableSuggestions = aliasSuggestions.filter(
    (suggestion) => !existingNames.has(suggestion.name)
  );
  const suggestionPageCount = Math.max(
    1,
    Math.ceil(availableSuggestions.length / suggestionPageSize)
  );
  state.suggestionPage = Math.min(state.suggestionPage, suggestionPageCount);
  const suggestionPageStart = (state.suggestionPage - 1) * suggestionPageSize;
  const visibleSuggestions = availableSuggestions.slice(
    suggestionPageStart,
    suggestionPageStart + suggestionPageSize
  );

  appElement.innerHTML = `
    <section class="shell">
      <header class="topbar">
        <div>
          <p class="eyebrow">macOS Alias Manager</p>
          <h1>EasyAlias</h1>
        </div>
        <div class="topbar-actions">
          <button
            class="header-icon-button"
            type="button"
            title="${state.appState.homeConnected ? "Import aliases from shell files" : "Connect Home folder"}"
            aria-label="${state.appState.homeConnected ? "Import aliases from shell files" : "Connect Home folder"}"
            data-action="open-import"
            ${state.importBusy ? "disabled" : ""}
          ><i data-lucide="square-terminal"></i></button>
          <button
            class="header-icon-button"
            type="button"
            title="Export alias backup"
            aria-label="Export alias backup"
            data-action="open-backup-export"
            ${aliases.length && !state.backupBusy ? "" : "disabled"}
          ><i data-lucide="file-up"></i></button>
          <button
            class="header-icon-button"
            type="button"
            title="Import alias backup"
            aria-label="Import alias backup"
            data-action="open-backup-import"
            ${state.backupBusy ? "disabled" : ""}
          ><i data-lucide="file-down"></i></button>
          <button
            class="header-icon-button trash-header-button"
            type="button"
            title="Trash${state.trashEntries.length ? ` (${state.trashEntries.length})` : ""}"
            aria-label="Open Trash${state.trashEntries.length ? ` with ${state.trashEntries.length} deleted aliases` : ""}"
            data-action="open-trash"
            ${state.trashBusy ? "disabled" : ""}
          >
            <i data-lucide="trash-2"></i>
            ${state.trashEntries.length ? `<span class="header-count" aria-hidden="true">${state.trashEntries.length}</span>` : ""}
          </button>
        </div>
      </header>

      <section class="status-grid">
        <div>
          <span>Alias Target</span>
          <strong title="${escapeHtml(state.appState.aliasTarget)}">${escapeHtml(state.appState.aliasTarget)}</strong>
        </div>
        <div>
          <span>Shell Files</span>
          <strong>${state.appState.homeConnected ? "Connected" : "Setup required"}</strong>
        </div>
        <div>
          <span>Aliases</span>
          <strong>${aliases.length}</strong>
        </div>
      </section>

      ${ 
        state.appState.homeConnected && state.appState.managedBlockPresent
          ? `<aside class="source-hint source-hint-connected">
              <span>EasyAlias only reads and updates .zshrc, .bash_profile, and .bashrc in the selected Home folder.</span>
              <button class="ghost-button compact-button" type="button" data-action="change-home">Change folder</button>
            </aside>`
          : `<aside class="source-hint">
              <span>${
                state.appState.connectionError
                  ? escapeHtml(state.appState.connectionError)
                  : state.appState.homeConnected
                    ? "One or more EasyAlias blocks are missing. Choose the Home folder again to restore them."
                    : "Choose your Home folder once so EasyAlias can manage zsh and Bash aliases within the App Sandbox."
              }</span>
              <button class="primary-button compact-button" type="button" data-action="choose-home">Choose Home folder</button>
              <small>Access is restricted by EasyAlias to .zshrc, .bash_profile, and .bashrc.</small>
            </aside>`
      }

      ${
        state.notice
          ? `<div class="message-banner notice" role="status">
              <span>${escapeHtml(state.notice)}</span>
              <button class="message-dismiss" type="button" title="Dismiss message" aria-label="Dismiss message" data-action="dismiss-message">
                <i data-lucide="x"></i>
              </button>
            </div>`
          : ""
      }
      ${
        state.error
          ? `<div class="message-banner error" role="alert">
              <span>${escapeHtml(state.error)}</span>
              <button class="message-dismiss" type="button" title="Dismiss message" aria-label="Dismiss message" data-action="dismiss-message">
                <i data-lucide="x"></i>
              </button>
            </div>`
          : ""
      }

      ${
        availableSuggestions.length
          ? `<section class="suggestions" data-expanded="${state.suggestionsExpanded}" aria-labelledby="suggestions-title">
              <div class="suggestions-header">
                <div class="suggestions-heading">
                  <h2 id="suggestions-title">Suggestions</h2>
                  <span>${availableSuggestions.length} available</span>
                </div>
                <button
                  class="suggestions-toggle"
                  type="button"
                  title="${state.suggestionsExpanded ? "Hide suggestions" : "Show suggestions"}"
                  aria-label="${state.suggestionsExpanded ? "Hide suggestions" : "Show suggestions"}"
                  aria-expanded="${state.suggestionsExpanded}"
                  aria-controls="suggestion-list"
                  data-action="toggle-suggestions"
                ><span aria-hidden="true">${state.suggestionsExpanded ? "⌄" : "›"}</span></button>
              </div>
              ${
                state.suggestionsExpanded
                  ? `<div id="suggestion-list">
                      <div class="suggestion-grid">
                      ${visibleSuggestions
                        .map(
                          (suggestion) => `
                            <article class="suggestion-item">
                              <div class="suggestion-copy">
                                <strong>${escapeHtml(suggestion.name)}</strong>
                                <span>${escapeHtml(suggestion.description)}</span>
                                <code>${escapeHtml(buildCommandPreview(suggestion))}</code>
                              </div>
                              <button
                                class="suggestion-button"
                                type="button"
                                data-action="use-suggestion"
                                data-suggestion-id="${suggestion.id}"
                                ${isTauriRuntime() && !state.appState.homeConnected ? "disabled" : ""}
                              >Use</button>
                            </article>
                          `
                        )
                        .join("")}
                      </div>
                      ${
                        suggestionPageCount > 1
                          ? `<nav class="suggestion-pagination" aria-label="Suggestion pages">
                              <button
                                class="suggestion-page-button suggestion-page-arrow"
                                type="button"
                                title="Previous suggestion page"
                                aria-label="Previous suggestion page"
                                data-action="suggestion-page"
                                data-page="${state.suggestionPage - 1}"
                                ${state.suggestionPage === 1 ? "disabled" : ""}
                              ><i data-lucide="chevron-left"></i></button>
                              ${Array.from({ length: suggestionPageCount }, (_, index) => index + 1)
                                .map(
                                  (page) => `<button
                                    class="suggestion-page-button${page === state.suggestionPage ? " is-current" : ""}"
                                    type="button"
                                    aria-label="Show suggestion page ${page}"
                                    ${page === state.suggestionPage ? 'aria-current="page"' : ""}
                                    data-action="suggestion-page"
                                    data-page="${page}"
                                  >${page}</button>`
                                )
                                .join("")}
                              <button
                                class="suggestion-page-button suggestion-page-arrow"
                                type="button"
                                title="Next suggestion page"
                                aria-label="Next suggestion page"
                                data-action="suggestion-page"
                                data-page="${state.suggestionPage + 1}"
                                ${state.suggestionPage === suggestionPageCount ? "disabled" : ""}
                              ><i data-lucide="chevron-right"></i></button>
                            </nav>`
                          : ""
                      }
                    </div>`
                  : ""
              }
            </section>`
          : ""
      }

      <section class="workspace">
        <form class="editor" id="alias-form">
          <div class="form-title">
            <h2>Create Alias</h2>
            <button class="primary-button" type="submit" ${
              isTauriRuntime() && !state.appState.homeConnected ? "disabled" : ""
            }>Add</button>
          </div>

          <label>
            Command Name
            <input name="name" value="${escapeHtml(state.form.name)}" placeholder="beerv2" autocomplete="off" />
          </label>

          <label>
            Location / File / Command
            <span class="path-picker-row">
              <input name="path" value="${escapeHtml(state.form.path)}" placeholder="~/Desktop/projects/beerv2_app" autocomplete="off" />
              <button class="picker-button" type="button" title="Choose file" data-action="pick-path" data-target="create" data-kind="file">File</button>
              <button class="picker-button" type="button" title="Choose folder" data-action="pick-path" data-target="create" data-kind="folder">Folder</button>
            </span>
          </label>

          <label>
            Action
            <select name="action">
              ${Object.entries(actionLabels)
                .map(
                  ([value, label]) =>
                    `<option value="${value}" ${state.form.action === value ? "selected" : ""}>${label}</option>`
                )
                .join("")}
            </select>
          </label>

          ${
            state.form.action === "custom"
              ? `<label>
                  Custom Command
                  <textarea name="customCommand" rows="4" placeholder='cd "$HOME/project" && ./run.sh'>${escapeHtml(state.form.customCommand)}</textarea>
                </label>`
              : ""
          }

          <div class="preview">
            <span>Preview</span>
            <code>${escapeHtml(formPreview())}</code>
          </div>
        </form>

        <section class="list" aria-label="Aliases">
          <div class="list-header">
            <h2>Your Aliases</h2>
            <span>${aliases.length} entries</span>
          </div>

          ${
            aliases.length
              ? aliases
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
                        <button class="edit-button" title="Edit" data-action="edit" data-id="${alias.id}">Edit</button>
                        <button class="icon-button" title="Delete" data-action="delete" data-id="${alias.id}">×</button>
                      </article>
                    `
                  )
                  .join("")
              : `<div class="empty-state">
                  <strong>No aliases yet</strong>
                  <span>Create your first command on the left.</span>
                </div>`
          }
        </section>
      </section>

      ${renderImportModal()}
      ${renderBackupDialog()}
      ${renderTrashDialog()}
      ${renderEditModal()}

      <footer class="app-footer">
        <a href="${repoUrl}" target="_blank" rel="noreferrer" data-external-link>
          © Hannes Gnann
        </a>
        <span aria-hidden="true">-</span>
        <a href="${redditUrl}" target="_blank" rel="noreferrer" data-external-link>
          Reddit
        </a>
        <span aria-hidden="true">-</span>
        <a href="${websiteUrl}" target="_blank" rel="noreferrer" data-external-link>
          Website
        </a>
      </footer>
    </section>
  `;

  // Replace the lightweight icon placeholders after each state-driven render.
  // Importing only the icons used here keeps the production bundle tree-shakable.
  createIcons({
    icons: {
      ChevronLeft,
      ChevronRight,
      SquareTerminal,
      FileDown,
      FileUp,
      RotateCcw,
      Star,
      Trash2,
      X
    },
    attrs: {
      "aria-hidden": "true",
      width: "20",
      height: "20",
      "stroke-width": "2"
    }
  });

  scheduleMessageDismissal();
  bindEvents();
}
