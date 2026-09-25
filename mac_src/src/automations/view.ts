// Automations view, cards, popovers and its event wiring.

import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Check,
  CircleStop,
  Clock,
  Clock3,
  FileDown,
  FileUp,
  Filter,
  FolderOpen,
  GraduationCap,
  Heart,
  Keyboard,
  LoaderCircle,
  Pencil,
  Play,
  Plus,
  RotateCcw,
  Save,
  Search,
  Settings,
  SquareTerminal,
  Star,
  Sunrise,
  Sunset,
  Tag,
  Tags,
  Terminal,
  Trash2,
  Workflow,
  X,
  createIcons
} from "lucide";
import { automationFilterLabels, redditUrl, repoUrl, sponsorUrl, websiteUrl } from "../constants";
import { replaceAppHtml } from "../a11y";
import { appElement } from "../dom";
import { escapeHtml } from "../html";
import { dismissMessage, scheduleMessageDismissal } from "../messages";
import {
  closeAllAutomationCardPopovers,
  closeAutomationsView,
  openSettingsView
} from "../navigation";
import { invokeCommand, isTauriRuntime, openExternalLink, openPathPicker } from "../platform";
import { acceleratorFromEvent, formatAccelerator } from "../preferences";
import { render } from "../render";
import { state } from "../state";
import { bindTutorialEvents, openTutorial, renderTutorialModal } from "../tutorial";
import type {
  Automation,
  AutomationCommandBehavior,
  AutomationFilter,
  AutomationStepKind,
  TimedAutomationTriggerKind
} from "../types";
import {
  assignAutomationGroup,
  deleteAutomation,
  saveAutomation,
  toggleAutomationFavorite,
  toggleAutomationGroupPicker
} from "./actions";
import {
  chooseAutomationBackupFile,
  closeAutomationBackupDialog,
  exportSelectedAutomations,
  importSelectedBackupAutomations,
  openAutomationBackupExport,
  openAutomationBackupImport,
  renderAutomationBackupDialog,
  syncAutomationBackupSelectionControls
} from "./backup";
import {
  addAutomationStep,
  automationStepLabel,
  closeAutomationEditor,
  moveAutomationStep,
  openAutomationEditor,
  persistAutomations,
  removeAutomationStep,
  renderAutomationEditor,
  updateAutomationEditor,
  updateAutomationStep
} from "./editor";
import {
  automationFilterLabel,
  automationGroups,
  automationOverviewCountLabel,
  compareAutomations,
  filterAutomations
} from "./filters";
import { closeAutomationRun, renderAutomationRun, runAutomation, stopAutomation } from "./run";
import {
  closeAutomationSchedulePicker,
  deleteAutomationSchedule,
  formatTimedAutomationDays,
  formatTimedAutomationTrigger,
  openAutomationSchedulePicker,
  renderAutomationScheduleModal,
  saveTimedAutomationEntry,
  setTimedAutomationEditorEnabled,
  setTimedAutomationEditorTriggerKind,
  setTimedAutomationEveryDay,
  toggleTimedAutomationDay,
  updateSunRegion,
  updateTimedAutomationEditorTime
} from "./schedule";
import {
  closeAutomationTrash,
  emptyAutomationTrash,
  openAutomationTrash,
  permanentlyDeleteTrashAutomation,
  renderAutomationTrashDialog,
  restoreTrashAutomation
} from "./trash";

// Search input stays mounted while only the result area is replaced. This
// avoids losing focus or jumping the caret while someone types quickly.
// Inline card popover for quickly assigning an automation to an existing
// group or creating a new one, without opening the full editor.
export function renderAutomationGroupPicker(automation: Automation, allAutomations: Automation[]) {
  const currentGroup = automation.group.trim();
  const existingGroups = automationGroups(allAutomations);

  return `<div class="automation-group-picker" role="group" aria-label="Assign ${escapeHtml(automation.name)} to a group">
      ${
        existingGroups.length
          ? `<div class="automation-group-picker-list">
              ${existingGroups
                .map(
                  (name) =>
                    `<button type="button" class="automation-group-option ${currentGroup === name ? "is-selected" : ""}" data-automation-action="assign-group" data-id="${escapeHtml(automation.id)}" data-group="${escapeHtml(name)}">${escapeHtml(name)}</button>`
                )
                .join("")}
            </div>`
          : `<p class="automation-group-picker-empty">No groups yet.</p>`
      }
      ${
        currentGroup
          ? `<button type="button" class="automation-group-option is-remove" data-automation-action="assign-group" data-id="${escapeHtml(automation.id)}" data-group="">No group</button>`
          : ""
      }
      <form class="automation-group-picker-create" data-automation-group-form="${escapeHtml(automation.id)}">
        <input type="text" name="automation-group-new" placeholder="New group name" maxlength="60" autocomplete="off" />
        <button class="ghost-button" type="submit">Create</button>
      </form>
    </div>`;
}

// Inline popover on an automation card for capturing / clearing its global
// keyboard shortcut. Mirrors the group picker: only one is open at a time and
// it collapses back into the card flow.
export function renderAutomationHotkeyPopover(automation: Automation) {
  const saved = automation.hotkey ?? "";
  const shown = state.hotkeyDraft ?? saved;
  const isDirty = state.hotkeyDraft !== null && state.hotkeyDraft !== saved;

  return `<div class="automation-hotkey-picker" role="group" aria-label="Keyboard shortcut for ${escapeHtml(automation.name)}">
      <button
        type="button"
        class="hotkey-capture ${shown ? "has-value" : ""}"
        data-automation-action="hotkey-capture"
        data-id="${escapeHtml(automation.id)}"
        aria-label="Press the keyboard shortcut for ${escapeHtml(automation.name)}"
      >${
        shown
          ? `<span class="hotkey-capture-combo">${escapeHtml(formatAccelerator(shown))}</span>`
          : `<span class="hotkey-capture-hint">Press a shortcut…</span>`
      }</button>
      ${
        state.hotkeyCaptureError
          ? `<p class="hotkey-capture-error" data-announce="assertive">${escapeHtml(state.hotkeyCaptureError)}</p>`
          : `<p class="hotkey-capture-help">Use at least one modifier, e.g. ⌘⇧L.</p>`
      }
      <div class="automation-hotkey-actions">
        ${
          saved
            ? `<button type="button" class="ghost-button" data-automation-action="hotkey-clear" data-id="${escapeHtml(automation.id)}" ${state.settingsBusy ? "disabled" : ""}>Remove</button>`
            : ""
        }
        <button type="button" class="ghost-button" data-automation-action="hotkey-cancel">Cancel</button>
        <button
          type="button"
          class="primary-button"
          data-automation-action="hotkey-save"
          data-id="${escapeHtml(automation.id)}"
          ${!isDirty || !state.hotkeyDraft || state.settingsBusy ? "disabled" : ""}
        ><i data-lucide="check"></i><span>Save</span></button>
      </div>
    </div>`;
}

export function toggleAutomationHotkeyPicker(id: string) {
  if (state.hotkeyEditorAutomationId === id) {
    state.hotkeyEditorAutomationId = null;
    state.hotkeyDraft = null;
    state.hotkeyCaptureError = "";
  } else {
    closeAllAutomationCardPopovers();
    state.hotkeyEditorAutomationId = id;
    state.hotkeyDraft = null;
    state.hotkeyCaptureError = "";
  }
  render();
}

export async function saveAutomationHotkey(id: string, accelerator: string | null) {
  const automation = state.automations.find((item) => item.id === id);
  if (!automation) return;

  if (!isTauriRuntime()) {
    // Browser preview has no OS registration - just mirror the value locally.
    const next = state.automations.map((item) =>
      item.id === id ? { ...item, hotkey: accelerator } : item
    );
    await persistAutomations(next);
    state.hotkeyEditorAutomationId = null;
    state.hotkeyDraft = null;
    state.hotkeyCaptureError = "";
    state.notice = accelerator
      ? `Shortcut ${formatAccelerator(accelerator)} assigned (preview only).`
      : "Shortcut removed.";
    render();
    return;
  }

  state.settingsBusy = true;
  state.hotkeyCaptureError = "";
  render();
  try {
    state.automations = await invokeCommand<Automation[]>("set_automation_hotkey", { id, accelerator });
    state.hotkeyEditorAutomationId = null;
    state.hotkeyDraft = null;
    state.notice = accelerator
      ? `Shortcut ${formatAccelerator(accelerator)} assigned.`
      : "Shortcut removed.";
    state.error = "";
  } catch (hotkeyError) {
    state.hotkeyCaptureError = String(hotkeyError);
  } finally {
    state.settingsBusy = false;
    render();
  }
}

export function renderAutomationGroupOverview(sortedAutomations: Automation[]) {
  const groupNames = automationGroups(sortedAutomations);
  const ungrouped = sortedAutomations.filter((automation) => !automation.group.trim());

  const cards = groupNames.map((name) => {
    const count = sortedAutomations.filter((automation) => automation.group.trim() === name).length;
    return `<button class="automation-group-card" type="button" data-automation-action="select-group" data-group="${escapeHtml(name)}">
        <i data-lucide="tag"></i>
        <span>${escapeHtml(name)}</span>
        <strong>${count} ${count === 1 ? "automation" : "automations"}</strong>
      </button>`;
  });

  if (ungrouped.length) {
    cards.push(`<button class="automation-group-card is-ungrouped" type="button" data-automation-action="select-group" data-group="">
        <i data-lucide="tag"></i>
        <span>Ungrouped</span>
        <strong>${ungrouped.length} ${ungrouped.length === 1 ? "automation" : "automations"}</strong>
      </button>`);
  }

  if (!cards.length) {
    return `<div class="automation-empty"><strong>No groups yet</strong><span>Add a group label to an automation to see it here.</span></div>`;
  }

  return `<div class="automation-group-grid">${cards.join("")}</div>`;
}

export function renderAutomationResults(sortedAutomations: Automation[]) {
  if (!sortedAutomations.length) {
    return `<div class="automation-empty"><div class="automation-empty-icon"><i data-lucide="play"></i></div><strong>No automations yet</strong><span>Combine project commands and waits into a repeatable workflow.</span><button class="primary-button" type="button" data-automation-action="new"><i data-lucide="plus"></i><span>Create automation</span></button></div>`;
  }

  if (state.automationFilter === "groups") {
    return renderAutomationGroupOverview(sortedAutomations);
  }

  const filteredAutomations = filterAutomations(sortedAutomations);

  if (!filteredAutomations.length) {
    return `<div class="automation-empty"><strong>No matching automations</strong><span>Try another search or filter.</span></div>`;
  }

  return `<div class="automation-grid">
      ${filteredAutomations
        .map((automation) => {
          const scheduleEntry = state.timedAutomations.find((entry) => entry.automationId === automation.id);
          const scheduleSummary = scheduleEntry
            ? `${formatTimedAutomationTrigger(scheduleEntry)} · ${formatTimedAutomationDays(scheduleEntry.days)}`
            : "";
          const scheduleTitle = scheduleEntry
            ? `Change schedule (currently ${scheduleSummary}${scheduleEntry.enabled ? "" : ", disabled"})`
            : "Schedule this automation";
          const hotkey = automation.hotkey ?? "";
          const hotkeyTitle = hotkey
            ? `Change keyboard shortcut (currently ${formatAccelerator(hotkey)})`
            : "Assign a keyboard shortcut";

          return `<article class="automation-card">
              <div class="automation-card-header">
                <div class="automation-card-title">
                  <button
                    class="automation-favorite-button ${automation.favorite ? "active" : ""}"
                    type="button"
                    title="${automation.favorite ? "Remove from favorites" : "Add to favorites"}"
                    aria-label="${automation.favorite ? "Remove" : "Add"} ${escapeHtml(automation.name)} ${automation.favorite ? "from" : "to"} favorites"
                    aria-pressed="${Boolean(automation.favorite)}"
                    data-automation-action="toggle-favorite"
                    data-id="${escapeHtml(automation.id)}"
                    ${state.automationRun?.running ? "disabled" : ""}
                  ><i data-lucide="star"></i></button>
                  <button
                    class="automation-group-button ${automation.group.trim() ? "has-group" : ""}"
                    type="button"
                    title="${automation.group.trim() ? `Change group (currently ${escapeHtml(automation.group.trim())})` : "Assign to a group"}"
                    aria-label="${automation.group.trim() ? "Change group for" : "Assign"} ${escapeHtml(automation.name)}${automation.group.trim() ? "" : " to a group"}"
                    aria-expanded="${state.automationGroupPickerId === automation.id}"
                    data-automation-action="toggle-group-picker"
                    data-id="${escapeHtml(automation.id)}"
                    ${state.automationRun?.running ? "disabled" : ""}
                  ><i data-lucide="tags"></i></button>
                  <button
                    class="automation-schedule-button ${scheduleEntry?.enabled ? "has-schedule" : ""}"
                    type="button"
                    title="${escapeHtml(scheduleTitle)}"
                    aria-label="${scheduleEntry ? "Change schedule for" : "Schedule"} ${escapeHtml(automation.name)}"
                    aria-expanded="${state.scheduleEditorAutomationId === automation.id}"
                    data-automation-action="toggle-schedule-picker"
                    data-id="${escapeHtml(automation.id)}"
                    ${state.automationRun?.running ? "disabled" : ""}
                  ><i data-lucide="clock"></i></button>
                  <button
                    class="automation-hotkey-button ${hotkey ? "has-hotkey" : ""}"
                    type="button"
                    title="${escapeHtml(hotkeyTitle)}"
                    aria-label="${hotkey ? "Change keyboard shortcut for" : "Assign keyboard shortcut to"} ${escapeHtml(automation.name)}"
                    aria-expanded="${state.hotkeyEditorAutomationId === automation.id}"
                    data-automation-action="toggle-hotkey-picker"
                    data-id="${escapeHtml(automation.id)}"
                    ${state.automationRun?.running ? "disabled" : ""}
                  ><i data-lucide="keyboard"></i></button>
                  <div>
                    <strong>${escapeHtml(automation.name)}</strong>
                    <code>${escapeHtml(automation.path)}</code>
                    <div class="automation-card-chips">
                      ${
                        automation.group.trim()
                          ? `<button class="automation-group-chip" type="button" title="Filter by group ${escapeHtml(automation.group.trim())}" data-automation-action="select-group" data-group="${escapeHtml(automation.group.trim())}"><i data-lucide="tag"></i><span>${escapeHtml(automation.group.trim())}</span></button>`
                          : ""
                      }
                      ${
                        hotkey
                          ? `<span class="automation-hotkey-chip" title="Keyboard shortcut ${escapeHtml(formatAccelerator(hotkey))}"><i data-lucide="keyboard"></i><span>${escapeHtml(formatAccelerator(hotkey))}</span></span>`
                          : ""
                      }
                    </div>
                  </div>
                </div>
                <span>${automation.steps.length} ${automation.steps.length === 1 ? "step" : "steps"}</span>
              </div>
              ${state.automationGroupPickerId === automation.id ? renderAutomationGroupPicker(automation, sortedAutomations) : ""}
              ${state.hotkeyEditorAutomationId === automation.id ? renderAutomationHotkeyPopover(automation) : ""}
              <ol class="automation-preview-list">
                ${automation.steps
                  .slice(0, 4)
                  .map((step) => `<li><span>${step.kind === "wait" ? "Wait" : step.behavior === "background" ? "Start" : "Run"}</span><code>${escapeHtml(automationStepLabel(step))}</code></li>`)
                  .join("")}
                ${automation.steps.length > 4 ? `<li class="automation-more">+ ${automation.steps.length - 4} more</li>` : ""}
              </ol>
              <div class="automation-card-actions">
                <button class="primary-button automation-run-button" type="button" data-automation-action="run" data-id="${escapeHtml(automation.id)}" ${state.automationRun?.running ? "disabled" : ""}><i data-lucide="play"></i><span>Run</span></button>
                <button class="header-icon-button" type="button" title="Edit ${escapeHtml(automation.name)}" aria-label="Edit ${escapeHtml(automation.name)}" data-automation-action="edit" data-id="${escapeHtml(automation.id)}" ${state.automationRun?.running ? "disabled" : ""}><i data-lucide="pencil"></i></button>
                <button class="header-icon-button automation-delete-button" type="button" title="Delete ${escapeHtml(automation.name)}" aria-label="Delete ${escapeHtml(automation.name)}" data-automation-action="delete" data-id="${escapeHtml(automation.id)}" ${state.automationRun?.running ? "disabled" : ""}><i data-lucide="trash-2"></i></button>
              </div>
            </article>`;
        })
        .join("")}
    </div>`;
}

export function refreshAutomationResults() {
  const sortedAutomations = [...state.automations].sort(compareAutomations);
  const count = document.querySelector<HTMLElement>("[data-automation-count]");
  const results = document.querySelector<HTMLElement>("[data-automation-results]");
  const filterLabel = document.querySelector<HTMLLabelElement>(".automation-filter");

  if (count) count.textContent = automationOverviewCountLabel(sortedAutomations);
  if (filterLabel) {
    filterLabel.classList.toggle("is-active", state.automationFilter !== "all");
    const select = filterLabel.querySelector("select");
    if (select) select.title = `Filter: ${automationFilterLabel(state.automationFilter)}`;
  }
  if (!results) return;

  results.innerHTML = renderAutomationResults(sortedAutomations);
  createIcons({
    icons: { Check, Clock, Keyboard, Pencil, Play, Plus, Star, Tag, Tags, Trash2 },
    attrs: { "aria-hidden": "true", width: "20", height: "20", "stroke-width": "2" }
  });
}

export function renderAutomationsView() {
  const sortedAutomations = [...state.automations].sort(compareAutomations);
  const automationGroupNames = automationGroups(sortedAutomations);
  replaceAppHtml(`
    <section class="shell automation-shell">
      <header class="topbar automation-topbar">
        <div>
          <p class="eyebrow">macOS Workflow Runner</p>
          <h1>Automations</h1>
        </div>
        <div class="topbar-actions">
          <button class="header-icon-button" type="button" title="Back to aliases" aria-label="Back to aliases" data-automation-action="back" ${state.automationRun?.running ? "disabled" : ""}><i data-lucide="arrow-left"></i></button>
          <button class="header-icon-button ${state.automationFilter === "groups" ? "active" : ""}" type="button" title="Group view" aria-label="Show automation groups" aria-pressed="${state.automationFilter === "groups"}" data-automation-action="show-groups" ${state.automationRun?.running ? "disabled" : ""}><i data-lucide="tags"></i></button>
          <button class="header-icon-button" type="button" title="Export automation backup" aria-label="Export automation backup" data-automation-action="open-backup-export" ${state.automations.length && !state.automationBackupBusy && !state.automationRun?.running ? "" : "disabled"}><i data-lucide="file-up"></i></button>
          <button class="header-icon-button" type="button" title="Import automation backup" aria-label="Import automation backup" data-automation-action="open-backup-import" ${state.automationBackupBusy || state.automationRun?.running ? "disabled" : ""}><i data-lucide="file-down"></i></button>
          <button
            class="header-icon-button trash-header-button"
            type="button"
            title="Automation Trash${state.automationTrashEntries.length ? ` (${state.automationTrashEntries.length})` : ""}"
            aria-label="Open Automation Trash${state.automationTrashEntries.length ? ` with ${state.automationTrashEntries.length} deleted automations` : ""}"
            data-automation-action="open-trash"
            ${state.automationTrashBusy || state.automationRun?.running ? "disabled" : ""}
          >
            <i data-lucide="trash-2"></i>
            ${state.automationTrashEntries.length ? `<span class="header-count" aria-hidden="true">${state.automationTrashEntries.length}</span>` : ""}
          </button>
          <button class="header-icon-button automation-create-button" type="button" title="Create automation" aria-label="Create automation" data-automation-action="new" ${state.automationRun?.running ? "disabled" : ""}><i data-lucide="plus"></i></button>
          <button class="header-icon-button" type="button" title="Settings" aria-label="Open settings" data-automation-action="open-settings" ${state.automationRun?.running ? "disabled" : ""}><i data-lucide="settings"></i></button>
          <button class="header-icon-button" type="button" title="Tutorial" aria-label="Open the tutorial" data-automation-action="open-tutorial"><i data-lucide="graduation-cap"></i></button>
        </div>
      </header>

      <section class="automation-summary">
        <div><span>Automations</span><strong>${state.automations.length}</strong></div>
        <div><span>Execution</span><strong>Sequential</strong></div>
        <div><span>Shell</span><strong>zsh</strong></div>
      </section>

      ${
        state.notice
          ? `<div class="message-banner notice"><span data-announce="polite">${escapeHtml(state.notice)}</span><button class="message-dismiss" type="button" title="Dismiss message" aria-label="Dismiss message" data-automation-action="dismiss-message"><i data-lucide="x"></i></button></div>`
          : ""
      }
      ${
        state.error
          ? `<div class="message-banner error"><span data-announce="assertive">${escapeHtml(state.error)}</span><button class="message-dismiss" type="button" title="Dismiss message" aria-label="Dismiss message" data-automation-action="dismiss-message"><i data-lucide="x"></i></button></div>`
          : ""
      }

      <section class="automation-overview">
        <div class="automation-overview-header">
          <div><h2>Your Automations</h2><span data-automation-count>${automationOverviewCountLabel(sortedAutomations)}</span></div>
          <button class="primary-button" type="button" data-automation-action="new" ${state.automationRun?.running ? "disabled" : ""}><i data-lucide="plus"></i><span>New automation</span></button>
        </div>
        <div class="automation-tools">
          <div class="automation-search" role="search">
            <i data-lucide="search"></i>
            <input
              type="search"
              name="automation-search"
              value="${escapeHtml(state.automationSearchQuery)}"
              placeholder="Search automations or commands"
              aria-label="Search automations by name, path, or command"
              autocomplete="off"
              ${state.automations.length ? "" : "disabled"}
            />
          </div>
          <label class="automation-filter ${state.automationFilter !== "all" ? "is-active" : ""} ${state.automations.length ? "" : "is-disabled"}">
            <span class="visually-hidden">Filter automations</span>
            <i data-lucide="filter"></i>
            <select
              name="automation-filter"
              aria-label="Filter automations"
              title="Filter: ${automationFilterLabel(state.automationFilter)}"
              ${state.automations.length ? "" : "disabled"}
            >
              ${Object.entries(automationFilterLabels)
                .map(
                  ([value, label]) =>
                    `<option value="${value}" ${state.automationFilter === value ? "selected" : ""}>${label}</option>`
                )
                .join("")}
              ${
                automationGroupNames.length
                  ? `<optgroup label="Groups">
                      ${automationGroupNames
                        .map(
                          (name) =>
                            `<option value="group:${escapeHtml(name)}" ${state.automationFilter === `group:${name}` ? "selected" : ""}>${escapeHtml(name)}</option>`
                        )
                        .join("")}
                    </optgroup>`
                  : ""
              }
            </select>
          </label>
        </div>
        <div class="automation-results" data-automation-results>
          ${renderAutomationResults(sortedAutomations)}
        </div>
      </section>

      ${renderAutomationEditor()}
      ${renderAutomationScheduleModal()}
      ${renderAutomationRun()}
      ${renderAutomationBackupDialog()}
      ${renderAutomationTrashDialog()}
      ${renderTutorialModal()}

      <aside class="support-banner" aria-label="Support EasyAlias"><span>Support EasyAlias development</span><a href="${sponsorUrl}" target="_blank" rel="noreferrer" data-external-link>❤ Become a sponsor</a><a class="support-star" href="${repoUrl}" target="_blank" rel="noreferrer" data-external-link>★ Give us a star on GitHub</a></aside>
      <footer class="app-footer"><a href="${repoUrl}" target="_blank" rel="noreferrer" data-external-link>© Hannes Gnann</a><span aria-hidden="true">-</span><a href="${redditUrl}" target="_blank" rel="noreferrer" data-external-link>Reddit</a><span aria-hidden="true">-</span><a href="${websiteUrl}" target="_blank" rel="noreferrer" data-external-link>Website</a></footer>
    </section>`);

  createIcons({
    icons: { ArrowDown, ArrowLeft, ArrowUp, Check, CircleStop, Clock, Clock3, FileDown, FileUp, Filter, FolderOpen, GraduationCap, Heart, Keyboard, LoaderCircle, Pencil, Play, Plus, RotateCcw, Save, Search, Settings, SquareTerminal, Star, Sunrise, Sunset, Tag, Tags, Terminal, Trash2, Workflow, X },
    attrs: { "aria-hidden": "true", width: "20", height: "20", "stroke-width": "2" }
  });
  scheduleMessageDismissal();
  bindAutomationEvents();
}

export function bindAutomationEvents() {
  document.querySelector<HTMLFormElement>("#automation-form")?.addEventListener("submit", saveAutomation);
  document.querySelector<HTMLFormElement>("#automation-backup-export-form")?.addEventListener("submit", exportSelectedAutomations);
  document.querySelector<HTMLFormElement>("#automation-backup-import-form")?.addEventListener("submit", importSelectedBackupAutomations);
  document.querySelectorAll<HTMLAnchorElement>("[data-external-link]").forEach((link) => link.addEventListener("click", openExternalLink));
  document.querySelector<HTMLInputElement>('input[name="automation-name"]')?.addEventListener("input", (event) => updateAutomationEditor("name", (event.target as HTMLInputElement).value));
  document.querySelector<HTMLInputElement>('input[name="automation-path"]')?.addEventListener("input", (event) => updateAutomationEditor("path", (event.target as HTMLInputElement).value));
  document.querySelector<HTMLInputElement>('input[name="automation-group"]')?.addEventListener("input", (event) => updateAutomationEditor("group", (event.target as HTMLInputElement).value));
  document.querySelectorAll<HTMLSelectElement>('select[name="automation-step-kind"]').forEach((select) => select.addEventListener("change", () => updateAutomationStep(Number(select.dataset.stepIndex), "kind", select.value as AutomationStepKind, true)));
  document.querySelectorAll<HTMLTextAreaElement>('textarea[name="automation-step-command"]').forEach((input) => input.addEventListener("input", () => updateAutomationStep(Number(input.dataset.stepIndex), "command", input.value)));
  document.querySelectorAll<HTMLSelectElement>('select[name="automation-step-behavior"]').forEach((select) => select.addEventListener("change", () => updateAutomationStep(Number(select.dataset.stepIndex), "behavior", select.value as AutomationCommandBehavior)));
  document.querySelectorAll<HTMLInputElement>('input[name="automation-step-seconds"]').forEach((input) => input.addEventListener("input", () => updateAutomationStep(Number(input.dataset.stepIndex), "seconds", Number(input.value))));

  document.querySelector<HTMLInputElement>('input[name="automation-backup-all"]')?.addEventListener("change", (event) => {
    const checked = (event.target as HTMLInputElement).checked;
    state.selectedAutomationBackupIds = checked
      ? new Set(state.automationBackupCandidates.map((automation) => automation.id))
      : new Set();
    document.querySelectorAll<HTMLInputElement>('input[name="automation-backup-candidate"]').forEach((checkbox) => {
      checkbox.checked = checked;
    });
    syncAutomationBackupSelectionControls();
  });

  document.querySelectorAll<HTMLInputElement>('input[name="automation-backup-candidate"]').forEach((checkbox) => {
    checkbox.addEventListener("change", () => {
      if (checkbox.checked) state.selectedAutomationBackupIds.add(checkbox.value);
      else state.selectedAutomationBackupIds.delete(checkbox.value);
      syncAutomationBackupSelectionControls();
    });
  });

  if (state.automationBackupDialogMode) syncAutomationBackupSelectionControls();

  document.querySelector<HTMLInputElement>('input[name="automation-search"]')?.addEventListener("input", (event) => {
    state.automationSearchQuery = (event.target as HTMLInputElement).value;
    refreshAutomationResults();
  });

  document.querySelector<HTMLSelectElement>('select[name="automation-filter"]')?.addEventListener("change", (event) => {
    state.automationFilter = (event.target as HTMLSelectElement).value as AutomationFilter;
    refreshAutomationResults();
  });

  // Card actions (favorite/run/edit/delete, plus "new" in the empty state) use
  // delegation on the results container so live search/filter can replace
  // those rows without rebinding handlers or disturbing the search field.
  document.querySelector<HTMLElement>("[data-automation-results]")?.addEventListener("click", (event) => {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-automation-action]");
    if (!button) return;

    const action = button.dataset.automationAction;
    const id = button.dataset.id;
    if (action === "new") openAutomationEditor();
    if (action === "edit" && id) openAutomationEditor(id);
    if (action === "delete" && id) void deleteAutomation(id);
    if (action === "toggle-favorite" && id) void toggleAutomationFavorite(id);
    if (action === "run" && id) void runAutomation(id);
    if (action === "select-group") {
      // A full render (not refreshAutomationResults) so the filter <select>
      // picks up this group as a newly selected option and its "is-active"
      // state, exactly as if the user had chosen it from the dropdown.
      state.automationFilter = `group:${button.dataset.group ?? ""}`;
      render();
    }
    if (action === "toggle-group-picker" && id) toggleAutomationGroupPicker(id);
    if (action === "assign-group" && id) void assignAutomationGroup(id, button.dataset.group ?? "");
    if (action === "toggle-schedule-picker" && id) openAutomationSchedulePicker(id);
    if (action === "toggle-hotkey-picker" && id) toggleAutomationHotkeyPicker(id);
    if (action === "hotkey-cancel") toggleAutomationHotkeyPicker(state.hotkeyEditorAutomationId ?? "");
    if (action === "hotkey-clear" && id) void saveAutomationHotkey(id, null);
    if (action === "hotkey-save" && id && state.hotkeyDraft) void saveAutomationHotkey(id, state.hotkeyDraft);
  });

  // The hotkey-capture button records the next key combo the user presses.
  document
    .querySelector<HTMLButtonElement>('.hotkey-capture[data-automation-action="hotkey-capture"]')
    ?.addEventListener("keydown", (event) => {
      // Tab keeps working so keyboard users can leave the recorder again.
      if (event.key === "Tab") return;
      event.preventDefault();
      if (event.key === "Escape") {
        toggleAutomationHotkeyPicker(state.hotkeyEditorAutomationId ?? "");
        return;
      }
      if (event.key === "Backspace" || event.key === "Delete") {
        state.hotkeyDraft = "";
        state.hotkeyCaptureError = "";
        render();
        return;
      }
      const accelerator = acceleratorFromEvent(event);
      if (!accelerator) {
        state.hotkeyCaptureError = "Add at least one modifier (⌘, ⌃, ⌥ or ⇧) plus another key.";
        render();
        return;
      }
      state.hotkeyDraft = accelerator;
      state.hotkeyCaptureError = "";
      render();
    });

  // The create-new-group mini form inside a card's group picker; submit
  // bubbles up to the same results container as the click delegation above.
  document.querySelector<HTMLElement>("[data-automation-results]")?.addEventListener("submit", (event) => {
    const form = (event.target as HTMLElement).closest<HTMLFormElement>("[data-automation-group-form]");
    if (!form) return;
    event.preventDefault();
    const id = form.dataset.automationGroupForm;
    const input = form.querySelector<HTMLInputElement>('input[name="automation-group-new"]');
    if (id && input) void assignAutomationGroup(id, input.value);
  });

  // The schedule modal - rendered once at the view level (not inside a
  // card), so its fields are bound directly here rather than delegated,
  // same as the main automation editor's fields just below.
  document.querySelector<HTMLFormElement>("#timed-automation-form")?.addEventListener("submit", saveTimedAutomationEntry);
  document.querySelector<HTMLInputElement>('input[name="timed-automation-time"]')?.addEventListener("input", (event) => {
    updateTimedAutomationEditorTime((event.target as HTMLInputElement).value);
  });
  document.querySelector<HTMLInputElement>('[data-timed-action="toggle-enabled"]')?.addEventListener("change", (event) => {
    setTimedAutomationEditorEnabled((event.target as HTMLInputElement).checked);
  });
  document.querySelector<HTMLSelectElement>('select[name="timed-automation-region"]')?.addEventListener("change", (event) => {
    void updateSunRegion((event.target as HTMLSelectElement).value);
  });
  document.querySelectorAll<HTMLButtonElement>("[data-timed-action]").forEach((button) => {
    button.addEventListener("click", () => {
      const action = button.dataset.timedAction;
      if (action === "close") closeAutomationSchedulePicker();
      if (action === "every-day") setTimedAutomationEveryDay();
      if (action === "toggle-day" && button.dataset.day) toggleTimedAutomationDay(button.dataset.day);
      if (action === "trigger-kind" && button.dataset.triggerKind) {
        setTimedAutomationEditorTriggerKind(button.dataset.triggerKind as TimedAutomationTriggerKind);
      }
      if (action === "delete" && state.scheduleEditorAutomationId) void deleteAutomationSchedule(state.scheduleEditorAutomationId);
    });
  });

  document.querySelectorAll<HTMLButtonElement>("[data-automation-action]").forEach((button) => {
    if (button.closest("[data-automation-results]")) return;
    button.addEventListener("click", () => {
      const action = button.dataset.automationAction;
      const id = button.dataset.id;
      const index = Number(button.dataset.stepIndex);
      if (action === "back") closeAutomationsView();
      if (action === "open-settings") openSettingsView();
      if (action === "open-tutorial") openTutorial();
      if (action === "open-backup-export") openAutomationBackupExport();
      if (action === "open-backup-import") openAutomationBackupImport();
      if (action === "close-backup") closeAutomationBackupDialog();
      if (action === "choose-backup-file") void chooseAutomationBackupFile();
      if (action === "open-trash") void openAutomationTrash();
      if (action === "close-trash") closeAutomationTrash();
      if (action === "restore-trash" && id) void restoreTrashAutomation(id);
      if (action === "delete-trash-permanently" && id) void permanentlyDeleteTrashAutomation(id);
      if (action === "empty-trash") void emptyAutomationTrash();
      if (action === "new") openAutomationEditor();
      if (action === "close-editor") closeAutomationEditor();
      if (action === "pick-folder") void openPathPicker("automation", "folder");
      if (action === "add-command") addAutomationStep("command");
      if (action === "add-wait") addAutomationStep("wait");
      if (action === "move-step") moveAutomationStep(index, Number(button.dataset.offset));
      if (action === "remove-step") removeAutomationStep(index);
      if (action === "stop-run") void stopAutomation();
      if (action === "close-run") closeAutomationRun();
      if (action === "dismiss-message") dismissMessage();
      if (action === "show-groups") {
        state.automationFilter = "groups";
        render();
      }
      if (action === "toggle-editor-group-picker") {
        state.automationEditorGroupPickerOpen = !state.automationEditorGroupPickerOpen;
        render();
      }
      if (action === "pick-editor-group") {
        updateAutomationEditor("group", button.dataset.group ?? "");
        state.automationEditorGroupPickerOpen = false;
        render();
      }
    });
  });

  // Keep focus on the shortcut-capture button across the re-renders each
  // keypress triggers, so the user can type the whole combo without re-clicking.
  if (state.hotkeyEditorAutomationId) {
    const capture = document.querySelector<HTMLButtonElement>(
      '.hotkey-capture[data-automation-action="hotkey-capture"]'
    );
    if (capture && document.activeElement !== capture) capture.focus();
  }

  bindTutorialEvents();
}
