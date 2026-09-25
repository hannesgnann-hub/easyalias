// Timed automations (clock time, sunrise, sunset).

import { formatDeletedDate } from "../aliases/trash";
import { weekdayLabels, weekdayOrder } from "../constants";
import { escapeHtml } from "../html";
import { saveBrowserSunLocation, saveBrowserTimedAutomations } from "../persistence";
import { createId, invokeCommand, isTauriRuntime, nowIso } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { SunLocationSetting, TimedAutomation, TimedAutomationTriggerKind } from "../types";

// Opens/closes an automation card's inline schedule popover - same pattern
// as toggleAutomationGroupPicker, so only one card's popover is open at a
// time and it collapses back into the card flow instead of a full page.
// Opens the schedule modal for one automation - a real modal (not an inline
// card popover) so its content never has to fit inside a grid cell, which
// is what caused the picker to overflow into neighboring cards.
export function openAutomationSchedulePicker(automationId: string) {
  const existing = state.timedAutomations.find((entry) => entry.automationId === automationId);
  const timestamp = nowIso();
  state.timedAutomationEditor = existing
    ? { ...existing }
    : {
        id: createId(),
        automationId,
        triggerKind: "clock",
        time: "09:00",
        days: [],
        enabled: true,
        createdAt: timestamp,
        updatedAt: timestamp,
        lastRunAt: null,
        lastRunStatus: null,
        lastRunOutput: null
      };
  state.scheduleEditorAutomationId = automationId;
  state.timedAutomationError = "";
  render();
}

export function closeAutomationSchedulePicker() {
  if (state.timedAutomationBusy) return;
  state.scheduleEditorAutomationId = null;
  state.timedAutomationEditor = null;
  state.timedAutomationError = "";
  render();
}

export function updateTimedAutomationEditorTime(time: string) {
  if (!state.timedAutomationEditor) return;
  state.timedAutomationEditor = { ...state.timedAutomationEditor, time };
  state.timedAutomationError = "";
}

export function setTimedAutomationEditorTriggerKind(triggerKind: TimedAutomationTriggerKind) {
  if (!state.timedAutomationEditor) return;
  state.timedAutomationEditor = { ...state.timedAutomationEditor, triggerKind };
  state.timedAutomationError = "";
  render();
}

// The sunrise/sunset region is a single global setting (every automation
// scheduled by the sun shares one physical location), so changing it saves
// right away instead of waiting for this automation's own Save button.
export async function updateSunRegion(region: string) {
  const nextLocation: SunLocationSetting = { region };
  try {
    if (isTauriRuntime()) {
      state.sunLocation = await invokeCommand<SunLocationSetting>("save_sun_location", { setting: nextLocation });
    } else {
      state.sunLocation = nextLocation;
      saveBrowserSunLocation();
    }
  } catch (saveError) {
    state.timedAutomationError = String(saveError);
  }
  render();
}

export function setTimedAutomationEditorEnabled(enabled: boolean) {
  if (!state.timedAutomationEditor) return;
  state.timedAutomationEditor = { ...state.timedAutomationEditor, enabled };
  render();
}

export function toggleTimedAutomationDay(day: string) {
  if (!state.timedAutomationEditor) return;
  const days = state.timedAutomationEditor.days.includes(day)
    ? state.timedAutomationEditor.days.filter((existing) => existing !== day)
    : [...state.timedAutomationEditor.days, day];
  state.timedAutomationEditor = { ...state.timedAutomationEditor, days };
  render();
}

export function setTimedAutomationEveryDay() {
  if (!state.timedAutomationEditor) return;
  state.timedAutomationEditor = { ...state.timedAutomationEditor, days: [] };
  render();
}

export function validateTimedAutomation(entry: TimedAutomation) {
  if (!entry.automationId) return "Choose an automation to schedule.";
  if (entry.triggerKind === "clock") {
    if (!entry.time) return "Choose a time.";
  } else if (!state.sunLocation.region) {
    return "Choose a region for sunrise/sunset scheduling.";
  }
  return "";
}

export async function saveTimedAutomationEntry(event: SubmitEvent) {
  event.preventDefault();
  if (!state.timedAutomationEditor || state.timedAutomationBusy) return;

  state.timedAutomationError = validateTimedAutomation(state.timedAutomationEditor);
  if (state.timedAutomationError) {
    render();
    return;
  }

  state.timedAutomationBusy = true;
  render();
  const savedEntry: TimedAutomation = { ...state.timedAutomationEditor, updatedAt: nowIso() };
  try {
    if (isTauriRuntime()) {
      state.timedAutomations = await invokeCommand<TimedAutomation[]>("save_timed_automation", { entry: savedEntry });
    } else {
      const exists = state.timedAutomations.some((item) => item.id === savedEntry.id);
      state.timedAutomations = exists
        ? state.timedAutomations.map((item) => (item.id === savedEntry.id ? savedEntry : item))
        : [...state.timedAutomations, savedEntry];
      saveBrowserTimedAutomations();
    }
    state.scheduleEditorAutomationId = null;
    state.timedAutomationEditor = null;
    state.notice = "Schedule saved.";
  } catch (saveError) {
    state.timedAutomationError = String(saveError);
  } finally {
    state.timedAutomationBusy = false;
    render();
  }
}

export async function deleteAutomationSchedule(automationId: string) {
  if (state.timedAutomationBusy) return;
  const entry = state.timedAutomations.find((item) => item.automationId === automationId);
  if (!entry) return;
  if (!window.confirm("Remove this automation's schedule?")) return;

  state.timedAutomationBusy = true;
  render();
  try {
    if (isTauriRuntime()) {
      state.timedAutomations = await invokeCommand<TimedAutomation[]>("delete_timed_automation", { id: entry.id });
    } else {
      state.timedAutomations = state.timedAutomations.filter((item) => item.id !== entry.id);
      saveBrowserTimedAutomations();
    }
    state.scheduleEditorAutomationId = null;
    state.timedAutomationEditor = null;
    state.notice = "Schedule removed.";
  } catch (deleteError) {
    state.error = String(deleteError);
  } finally {
    state.timedAutomationBusy = false;
    render();
  }
}

export function formatTimedAutomationDays(days: string[]) {
  if (!days.length) return "Every day";
  return weekdayOrder
    .filter((day) => days.includes(day))
    .map((day) => weekdayLabels[day])
    .join(", ");
}

export function formatTimedAutomationTrigger(entry: TimedAutomation) {
  if (entry.triggerKind === "sunrise") return "Sunrise";
  if (entry.triggerKind === "sunset") return "Sunset";
  return entry.time;
}

// Modal for scheduling one automation to run at a wall-clock time
// (optionally on specific weekdays). Rendered once at the view level (like
// the main automation editor modal), not inside a card, so its content
// never has to squeeze into a grid cell.
export function renderAutomationScheduleModal() {
  if (!state.timedAutomationEditor || !state.scheduleEditorAutomationId) return "";
  const automation = state.automations.find((item) => item.id === state.scheduleEditorAutomationId);
  if (!automation) return "";
  const entry = state.timedAutomationEditor;
  const existingEntry = state.timedAutomations.find((item) => item.automationId === automation.id);

  return `
    <section class="modal-layer" role="dialog" aria-modal="true" aria-labelledby="timed-automation-editor-title" data-dialog="automation-schedule">
      <form class="modal-card automation-editor" id="timed-automation-form">
        <div class="modal-title">
          <div>
            <p class="eyebrow">Schedule</p>
            <h2 id="timed-automation-editor-title">${escapeHtml(automation.name)}</h2>
          </div>
          <button class="ghost-button modal-close" type="button" data-timed-action="close" ${state.timedAutomationBusy ? "disabled" : ""}>Close</button>
        </div>

        <p class="automation-intro">Runs through the operating system's own scheduler, so it fires even while EasyAlias is closed.</p>
        ${state.timedAutomationError ? `<p class="modal-error" data-announce="assertive">${escapeHtml(state.timedAutomationError)}</p>` : ""}

        <label class="timed-automation-toggle">
          <input type="checkbox" ${entry.enabled ? "checked" : ""} data-timed-action="toggle-enabled" ${state.timedAutomationBusy ? "disabled" : ""} />
          <span>${entry.enabled ? "Enabled" : "Disabled"}</span>
        </label>

        <div class="timed-automation-trigger-kind">
          <span class="automation-optional">When</span>
          <div class="timed-automation-day-chips">
            <button type="button" class="timed-automation-day-chip ${entry.triggerKind === "clock" ? "is-selected" : ""}" data-timed-action="trigger-kind" data-trigger-kind="clock" ${state.timedAutomationBusy ? "disabled" : ""}><i data-lucide="clock"></i><span>Time</span></button>
            <button type="button" class="timed-automation-day-chip ${entry.triggerKind === "sunrise" ? "is-selected" : ""}" data-timed-action="trigger-kind" data-trigger-kind="sunrise" ${state.timedAutomationBusy ? "disabled" : ""}><i data-lucide="sunrise"></i><span>Sunrise</span></button>
            <button type="button" class="timed-automation-day-chip ${entry.triggerKind === "sunset" ? "is-selected" : ""}" data-timed-action="trigger-kind" data-trigger-kind="sunset" ${state.timedAutomationBusy ? "disabled" : ""}><i data-lucide="sunset"></i><span>Sunset</span></button>
          </div>
        </div>

        ${
          entry.triggerKind === "clock"
            ? `<label>
                Time
                <input type="time" name="timed-automation-time" value="${escapeHtml(entry.time)}" ${state.timedAutomationBusy ? "disabled" : ""} />
              </label>`
            : `<label>
                Region (for computing today's ${entry.triggerKind})
                <select name="timed-automation-region" ${state.timedAutomationBusy ? "disabled" : ""}>
                  ${state.sunRegionOptions
                    .map(
                      (option) =>
                        `<option value="${escapeHtml(option.value)}" ${state.sunLocation.region === option.value ? "selected" : ""}>${escapeHtml(option.label)}</option>`
                    )
                    .join("")}
                </select>
              </label>`
        }

        <div class="timed-automation-days">
          <span class="automation-optional">Repeat</span>
          <div class="timed-automation-day-chips">
            <button type="button" class="timed-automation-day-chip ${entry.days.length === 0 ? "is-selected" : ""}" data-timed-action="every-day" ${state.timedAutomationBusy ? "disabled" : ""}>Every day</button>
            ${weekdayOrder
              .map(
                (day) =>
                  `<button type="button" class="timed-automation-day-chip ${entry.days.includes(day) ? "is-selected" : ""}" data-timed-action="toggle-day" data-day="${day}" ${state.timedAutomationBusy ? "disabled" : ""}>${weekdayLabels[day]}</button>`
              )
              .join("")}
          </div>
        </div>

        ${
          existingEntry?.lastRunAt
            ? `<span class="timed-automation-last-run is-${existingEntry.lastRunStatus ?? "unknown"}">Last run ${formatDeletedDate(existingEntry.lastRunAt)} · ${existingEntry.lastRunStatus === "error" ? "failed" : "succeeded"}</span>`
            : ""
        }
        ${
          existingEntry?.lastRunStatus === "error" && existingEntry.lastRunOutput
            ? `<pre class="timed-automation-error-output">${escapeHtml(existingEntry.lastRunOutput)}</pre>`
            : ""
        }

        <div class="modal-actions">
          ${
            existingEntry
              ? `<button class="ghost-button automation-delete-button" type="button" data-timed-action="delete" ${state.timedAutomationBusy ? "disabled" : ""}>Remove schedule</button>`
              : `<button class="ghost-button" type="button" data-timed-action="close" ${state.timedAutomationBusy ? "disabled" : ""}>Cancel</button>`
          }
          <button class="primary-button" type="submit" ${state.timedAutomationBusy ? "disabled" : ""}><i data-lucide="save"></i><span>${state.timedAutomationBusy ? "Saving..." : "Save"}</span></button>
        </div>
      </form>
    </section>`;
}
