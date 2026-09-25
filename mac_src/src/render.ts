// Top-level render of the alias view and shared modals.

import {
  ChevronLeft,
  ChevronRight,
  FileDown,
  FileUp,
  Filter,
  GraduationCap,
  Heart,
  Pencil,
  Play,
  RotateCcw,
  Search,
  Settings,
  SquareTerminal,
  Star,
  Trash2,
  Workflow,
  X,
  createIcons
} from "lucide";
import { renderBackupDialog } from "./aliases/backup";
import { buildCommandPreview } from "./aliases/command";
import { renderEditModal } from "./aliases/edit";
import {
  aliasCountLabel,
  compareAliases,
  filterAliases,
  formPreview,
  renderAliasResults
} from "./aliases/list";
import { renderImportModal } from "./aliases/shellImport";
import { aliasSuggestions } from "./aliases/suggestions";
import { renderTrashDialog } from "./aliases/trash";
import { renderAutomationsView } from "./automations/view";
import {
  actionLabels,
  aliasFilterLabels,
  redditUrl,
  repoUrl,
  sponsorUrl,
  websiteUrl
} from "./constants";
import { replaceAppHtml } from "./a11y";
import { appElement } from "./dom";
import { bindEvents } from "./events";
import { escapeHtml } from "./html";
import { scheduleMessageDismissal } from "./messages";
import { renderSettingsView } from "./settings/view";
import { state, suggestionPageSize } from "./state";
import { renderTutorialModal } from "./tutorial";

// Main render function. This replaces the app HTML from state and then calls bindEvents().
// For a larger app, this would be a good candidate to split into smaller render helpers.
export function render() {
  if (state.currentView === "settings") {
    renderSettingsView();
    return;
  }
  if (state.currentView === "automations") {
    renderAutomationsView();
    return;
  }
  const aliases = [...state.appState.aliases].sort(compareAliases);
  const filteredAliasCount = filterAliases(aliases).length;
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

  replaceAppHtml(`
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
            title="Open automations"
            aria-label="Open automations"
            data-action="open-automations"
          ><i data-lucide="play"></i></button>
          <button
            class="header-icon-button"
            type="button"
            title="Import aliases from ${escapeHtml(state.appState.shellConfigFile)}"
            aria-label="Import aliases from ${escapeHtml(state.appState.shellConfigFile)}"
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
          <button
            class="header-icon-button"
            type="button"
            title="Settings"
            aria-label="Open settings"
            data-action="open-settings"
          ><i data-lucide="settings"></i></button>
          <button
            class="header-icon-button"
            type="button"
            title="Tutorial"
            aria-label="Open the tutorial"
            data-action="open-tutorial"
          ><i data-lucide="graduation-cap"></i></button>
        </div>
      </header>

      <section class="status-grid">
        <div>
          <span>Alias File</span>
          <strong>${state.appState.aliasesFile}</strong>
        </div>
        <div>
          <span>${escapeHtml(state.appState.shellName)} Source</span>
          <strong>${state.appState.shellSourcePresent ? "Connected" : "Not connected yet"}</strong>
        </div>
        <div>
          <span>Aliases</span>
          <strong>${aliases.length}</strong>
        </div>
      </section>

      ${
        state.appState.shellSourcePresent
          ? ""
          : `<aside class="source-hint">
              <span>Automatically added to ${escapeHtml(state.appState.shellConfigFile)} on first startup:</span>
              <code>${state.appState.sourceLine}</code>
            </aside>`
      }

      ${
        state.notice
          ? `<div class="message-banner notice">
              <span data-announce="polite">${escapeHtml(state.notice)}</span>
              <button class="message-dismiss" type="button" title="Dismiss message" aria-label="Dismiss message" data-action="dismiss-message">
                <i data-lucide="x"></i>
              </button>
            </div>`
          : ""
      }
      ${
        state.error
          ? `<div class="message-banner error">
              <span data-announce="assertive">${escapeHtml(state.error)}</span>
              <button class="message-dismiss" type="button" title="Dismiss message" aria-label="Dismiss message" data-action="dismiss-message">
                <i data-lucide="x"></i>
              </button>
            </div>`
          : ""
      }

      ${
        state.appSettings.showSuggestions && availableSuggestions.length
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
            <button class="primary-button" type="submit">Add</button>
          </div>

          <label>
            Command Name
            <input name="name" value="${escapeHtml(state.form.name)}" placeholder="myproject" autocomplete="off" />
          </label>

          <label>
            Location / File / Command
            <span class="path-picker-row">
              <input name="path" value="${escapeHtml(state.form.path)}" placeholder="~/Projects/my-app" autocomplete="off" />
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
            <span data-alias-count>${aliasCountLabel(aliases.length, filteredAliasCount)}</span>
          </div>
          <div class="alias-tools">
            <div class="alias-search" role="search">
              <i data-lucide="search"></i>
              <input
                type="search"
                name="alias-search"
                value="${escapeHtml(state.aliasSearchQuery)}"
                placeholder="Search aliases or commands"
                aria-label="Search aliases by name or command"
                autocomplete="off"
                ${aliases.length ? "" : "disabled"}
              />
            </div>
            <label class="alias-filter ${state.aliasFilter !== "all" ? "is-active" : ""} ${aliases.length ? "" : "is-disabled"}">
              <span class="visually-hidden">Filter aliases</span>
              <i data-lucide="filter"></i>
              <select
                name="alias-filter"
                aria-label="Filter aliases"
                title="Filter: ${aliasFilterLabels[state.aliasFilter]}"
                ${aliases.length ? "" : "disabled"}
              >
                ${Object.entries(aliasFilterLabels)
                  .map(
                    ([value, label]) =>
                      `<option value="${value}" ${state.aliasFilter === value ? "selected" : ""}>${label}</option>`
                  )
                  .join("")}
              </select>
            </label>
          </div>
          <div class="alias-results" data-alias-results>
            ${renderAliasResults(aliases)}
          </div>
        </section>
      </section>

      ${renderImportModal()}
      ${renderBackupDialog()}
      ${renderTrashDialog()}
      ${renderEditModal()}
      ${renderTutorialModal()}

      <aside class="support-banner" aria-label="Support EasyAlias">
        <span>Support EasyAlias development</span>
        <a href="${sponsorUrl}" target="_blank" rel="noreferrer" data-external-link>
          ❤ Become a sponsor
        </a>
        <a class="support-star" href="${repoUrl}" target="_blank" rel="noreferrer" data-external-link>
          ★ Give us a star on GitHub
        </a>
      </aside>

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
  `);

  // Replace the lightweight icon placeholders after each state-driven render.
  // Importing only the icons used here keeps the production bundle tree-shakable.
  createIcons({
    icons: {
      ChevronLeft,
      ChevronRight,
      GraduationCap,
      Heart,
      SquareTerminal,
      FileDown,
      FileUp,
      Filter,
      Pencil,
      Play,
      RotateCcw,
      Search,
      Settings,
      Star,
      Trash2,
      Workflow,
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
