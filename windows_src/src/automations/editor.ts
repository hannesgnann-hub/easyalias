// Automation editor modal.

import { escapeHtml } from "../html";
import { createId, invokeCommand, isTauriRuntime, nowIso } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { Automation, AutomationStep, AutomationStepKind } from "../types";
import { automationGroups } from "./filters";

export function createAutomationStep(kind: AutomationStepKind): AutomationStep {
  return {
    id: createId(),
    kind,
    command: "",
    seconds: kind === "wait" ? 10 : 0,
    behavior: "wait"
  };
}

export function openAutomationEditor(id?: string) {
  const existing = id ? state.automations.find((automation) => automation.id === id) : null;
  const timestamp = nowIso();
  state.automationEditor = existing
    ? {
        ...existing,
        steps: existing.steps.map((step) => ({ ...step }))
      }
    : {
        id: createId(),
        name: "",
        path: "~/Projects",
        steps: [createAutomationStep("command")],
        favorite: false,
        group: "",
        createdAt: timestamp,
        updatedAt: timestamp
      };
  state.automationError = "";
  state.automationEditorGroupPickerOpen = false;
  render();
}

export function closeAutomationEditor() {
  if (state.automationBusy) return;
  state.automationEditor = null;
  state.automationError = "";
  state.automationEditorGroupPickerOpen = false;
  render();
}

export function updateAutomationEditor<K extends "name" | "path" | "group">(key: K, value: Automation[K]) {
  if (!state.automationEditor) return;
  state.automationEditor = { ...state.automationEditor, [key]: value };
  state.automationError = "";
  document.querySelector(".automation-editor .modal-error")?.remove();
}

export function updateAutomationStep<K extends keyof AutomationStep>(
  index: number,
  key: K,
  value: AutomationStep[K],
  rerender = false
) {
  if (!state.automationEditor || !state.automationEditor.steps[index]) return;
  const steps = state.automationEditor.steps.map((step, stepIndex) =>
    stepIndex === index ? { ...step, [key]: value } : step
  );
  state.automationEditor = { ...state.automationEditor, steps };
  state.automationError = "";
  if (rerender) render();
}

export function addAutomationStep(kind: AutomationStepKind) {
  if (!state.automationEditor) return;
  state.automationEditor = {
    ...state.automationEditor,
    steps: [...state.automationEditor.steps, createAutomationStep(kind)]
  };
  state.automationError = "";
  render();
}

export function moveAutomationStep(index: number, offset: number) {
  if (!state.automationEditor) return;
  const target = index + offset;
  if (target < 0 || target >= state.automationEditor.steps.length) return;
  const steps = [...state.automationEditor.steps];
  [steps[index], steps[target]] = [steps[target], steps[index]];
  state.automationEditor = { ...state.automationEditor, steps };
  render();
}

export function removeAutomationStep(index: number) {
  if (!state.automationEditor || state.automationEditor.steps.length === 1) {
    state.automationError = "An automation needs at least one step.";
    render();
    return;
  }
  state.automationEditor = {
    ...state.automationEditor,
    steps: state.automationEditor.steps.filter((_, stepIndex) => stepIndex !== index)
  };
  state.automationError = "";
  render();
}

export function validateAutomation(automation: Automation) {
  if (!automation.name.trim()) return "Enter a name for the automation.";
  if (!automation.path.trim()) return "Choose a working directory.";
  if (automation.group.trim().length > 60) return "The group label must be at most 60 characters.";
  if (!automation.steps.length) return "Add at least one step.";

  for (const [index, step] of automation.steps.entries()) {
    if (step.kind === "command" && !step.command.trim()) {
      return `Step ${index + 1} needs a command.`;
    }
    if (step.kind === "wait" && (!Number.isFinite(step.seconds) || step.seconds < 1 || step.seconds > 86400)) {
      return `Step ${index + 1} must wait between 1 second and 24 hours.`;
    }
  }

  return "";
}

export async function persistAutomations(next: Automation[]) {
  if (isTauriRuntime()) {
    state.automations = await invokeCommand<Automation[]>("save_automations", { automations: next });
    return;
  }
  state.automations = next;
  localStorage.setItem("easyalias-automations", JSON.stringify(next));
}

export function automationStepLabel(step: AutomationStep) {
  if (step.kind === "wait") return `Wait ${step.seconds}s`;
  return step.command || "Command not configured";
}

export function renderAutomationEditor() {
  if (!state.automationEditor) return "";

  return `
    <section class="modal-layer" role="presentation">
      <form class="modal-card automation-editor" id="automation-form" role="dialog" aria-modal="true" aria-labelledby="automation-editor-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">Workflow</p>
            <h2 id="automation-editor-title">${escapeHtml(state.automationEditor.name || "New automation")}</h2>
          </div>
          <button class="ghost-button modal-close" type="button" data-automation-action="close-editor" ${state.automationBusy ? "disabled" : ""}>Close</button>
        </div>

        <p class="automation-intro">Commands run from top to bottom in the same working directory. Background commands let long-running development servers start without blocking the next step.</p>
        ${state.automationError ? `<p class="modal-error">${escapeHtml(state.automationError)}</p>` : ""}

        <div class="automation-form-grid">
          <label>
            Name
            <input name="automation-name" value="${escapeHtml(state.automationEditor.name)}" placeholder="Development workflow" autocomplete="off" />
          </label>
          <label>
            Working Directory
            <span class="automation-path-row">
              <input name="automation-path" value="${escapeHtml(state.automationEditor.path)}" placeholder="~/Projects/my-app" autocomplete="off" />
              <button class="picker-button automation-folder-button" type="button" title="Choose working directory" aria-label="Choose working directory" data-automation-action="pick-folder"><i data-lucide="folder-open"></i></button>
            </span>
          </label>
          <label>
            Group <span class="automation-optional">(optional)</span>
            <span class="automation-path-row">
              <input name="automation-group" value="${escapeHtml(state.automationEditor.group)}" placeholder="e.g. Backend, or type a new name" autocomplete="off" maxlength="60" />
              <button class="picker-button automation-group-picker-button" type="button" title="Choose an existing group" aria-label="Choose an existing group" aria-expanded="${state.automationEditorGroupPickerOpen}" data-automation-action="toggle-editor-group-picker"><i data-lucide="tags"></i></button>
            </span>
            ${
              state.automationEditorGroupPickerOpen
                ? `<div class="automation-group-picker automation-group-picker-editor" role="menu" aria-label="Existing groups">
                    ${
                      automationGroups(state.automations).length
                        ? automationGroups(state.automations)
                            .map(
                              (name) =>
                                `<button type="button" class="automation-group-option ${state.automationEditor!.group.trim() === name ? "is-selected" : ""}" data-automation-action="pick-editor-group" data-group="${escapeHtml(name)}">${escapeHtml(name)}</button>`
                            )
                            .join("")
                        : `<p class="automation-group-picker-empty">No groups yet. Type a new name above.</p>`
                    }
                  </div>`
                : ""
            }
          </label>
        </div>

        <div class="automation-step-heading">
          <div>
            <h3>Steps</h3>
            <span>${state.automationEditor.steps.length} configured</span>
          </div>
          <div class="automation-add-actions">
            <button class="ghost-button" type="button" data-automation-action="add-command"><i data-lucide="terminal"></i><span>Add command</span></button>
            <button class="ghost-button" type="button" data-automation-action="add-wait"><i data-lucide="clock-3"></i><span>Add wait</span></button>
          </div>
        </div>

        <div class="automation-step-list">
          ${state.automationEditor.steps
            .map(
              (step, index) => `
                <article class="automation-step-editor" data-step-index="${index}">
                  <div class="automation-step-number">${index + 1}</div>
                  <div class="automation-step-fields">
                    <label>
                      Step Type
                      <select name="automation-step-kind" data-step-index="${index}">
                        <option value="command" ${step.kind === "command" ? "selected" : ""}>Command</option>
                        <option value="wait" ${step.kind === "wait" ? "selected" : ""}>Wait</option>
                      </select>
                    </label>
                    ${
                      step.kind === "command"
                        ? `<label class="automation-command-field">
                            Command
                            <textarea name="automation-step-command" data-step-index="${index}" rows="2" placeholder="npm run dev">${escapeHtml(step.command)}</textarea>
                          </label>
                          <label>
                            Continue When
                            <select name="automation-step-behavior" data-step-index="${index}">
                              <option value="wait" ${step.behavior === "wait" ? "selected" : ""}>Command finishes</option>
                              <option value="background" ${step.behavior === "background" ? "selected" : ""}>Process starts</option>
                            </select>
                          </label>`
                        : `<label class="automation-wait-field">
                            Seconds
                            <input name="automation-step-seconds" data-step-index="${index}" type="number" min="1" max="86400" step="1" value="${step.seconds}" />
                          </label>`
                    }
                  </div>
                  <div class="automation-step-controls">
                    <button type="button" title="Move step up" aria-label="Move step ${index + 1} up" data-automation-action="move-step" data-step-index="${index}" data-offset="-1" ${index === 0 ? "disabled" : ""}><i data-lucide="arrow-up"></i></button>
                    <button type="button" title="Move step down" aria-label="Move step ${index + 1} down" data-automation-action="move-step" data-step-index="${index}" data-offset="1" ${index === state.automationEditor!.steps.length - 1 ? "disabled" : ""}><i data-lucide="arrow-down"></i></button>
                    <button class="danger" type="button" title="Remove step" aria-label="Remove step ${index + 1}" data-automation-action="remove-step" data-step-index="${index}"><i data-lucide="trash-2"></i></button>
                  </div>
                </article>`
            )
            .join("")}
        </div>

        <div class="modal-actions">
          <button class="ghost-button" type="button" data-automation-action="close-editor" ${state.automationBusy ? "disabled" : ""}>Cancel</button>
          <button class="primary-button" type="submit" ${state.automationBusy ? "disabled" : ""}><i data-lucide="save"></i><span>${state.automationBusy ? "Saving..." : "Save automation"}</span></button>
        </div>
      </form>
    </section>`;
}
