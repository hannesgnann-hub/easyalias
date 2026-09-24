// Saving, deleting, favoriting and grouping automations.

import { saveBrowserAutomationTrash } from "../persistence";
import { invokeCommand, isTauriRuntime, nowIso } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { Automation, AutomationTrashMutationResult } from "../types";
import { persistAutomations, validateAutomation } from "./editor";
import { refreshAutomationResults } from "./view";

export async function saveAutomation(event: SubmitEvent) {
  event.preventDefault();
  if (!state.automationEditor || state.automationBusy) return;

  state.automationError = validateAutomation(state.automationEditor);
  if (state.automationError) {
    render();
    return;
  }
  const duplicate = state.automations.find(
    (automation) =>
      automation.id !== state.automationEditor?.id &&
      automation.name.trim().toLocaleLowerCase() === state.automationEditor?.name.trim().toLocaleLowerCase()
  );
  if (duplicate) {
    state.automationError = `Automation "${state.automationEditor.name.trim()}" already exists.`;
    render();
    return;
  }

  state.automationBusy = true;
  render();
  const savedAutomation: Automation = {
    ...state.automationEditor,
    name: state.automationEditor.name.trim(),
    path: state.automationEditor.path.trim(),
    group: state.automationEditor.group.trim(),
    steps: state.automationEditor.steps.map((step) => ({
      ...step,
      command: step.kind === "command" ? step.command.trim() : "",
      seconds: step.kind === "wait" ? Math.floor(step.seconds) : 0
    })),
    updatedAt: nowIso()
  };
  const exists = state.automations.some((automation) => automation.id === savedAutomation.id);
  const next = exists
    ? state.automations.map((automation) =>
        automation.id === savedAutomation.id ? savedAutomation : automation
      )
    : [...state.automations, savedAutomation];

  try {
    await persistAutomations(next);
    state.automationEditor = null;
    state.notice = `Automation "${savedAutomation.name}" saved.`;
  } catch (saveError) {
    state.automationError = String(saveError);
  } finally {
    state.automationBusy = false;
    render();
  }
}

export async function deleteAutomation(id: string) {
  if (state.automationBusy || state.automationRun?.running) return;
  const automation = state.automations.find((item) => item.id === id);
  if (!automation || !window.confirm(`Move automation "${automation.name}" to Trash?`)) return;
  state.automationBusy = true;
  try {
    if (isTauriRuntime()) {
      const result = await invokeCommand<AutomationTrashMutationResult>("move_automation_to_trash", { id });
      state.automations = result.automations;
      state.automationTrashEntries = result.trash;
    } else {
      const nextAutomations = state.automations.filter((item) => item.id !== id);
      await persistAutomations(nextAutomations);
      state.automationTrashEntries = [
        { automation, deletedAt: Math.floor(Date.now() / 1000) },
        ...state.automationTrashEntries.filter((entry) => entry.automation.id !== id)
      ].sort((left, right) => right.deletedAt - left.deletedAt);
      saveBrowserAutomationTrash();
    }
    state.notice = `Automation "${automation.name}" moved to Trash.`;
  } catch (deleteError) {
    state.error = String(deleteError);
  } finally {
    state.automationBusy = false;
    render();
  }
}

export async function toggleAutomationFavorite(id: string) {
  if (state.automationBusy || state.automationRun?.running) return;
  const automation = state.automations.find((item) => item.id === id);
  if (!automation) return;

  state.automationBusy = true;
  try {
    await persistAutomations(
      state.automations.map((item) =>
        item.id === id
          ? { ...item, favorite: !Boolean(item.favorite), updatedAt: nowIso() }
          : item
      )
    );
  } catch (favoriteError) {
    state.error = String(favoriteError);
  } finally {
    state.automationBusy = false;
    render();
  }
}

export function toggleAutomationGroupPicker(id: string) {
  state.automationGroupPickerId = state.automationGroupPickerId === id ? null : id;
  // Only one card popover (group or schedule) is open at a time.
  state.scheduleEditorAutomationId = null;
  state.timedAutomationEditor = null;
  refreshAutomationResults();
}

export async function assignAutomationGroup(id: string, group: string) {
  const automation = state.automations.find((item) => item.id === id);
  if (!automation) return;

  const trimmedGroup = group.trim();
  if (trimmedGroup.length > 60) {
    state.error = "The group label must be at most 60 characters.";
    render();
    return;
  }

  state.automationGroupPickerId = null;
  try {
    await persistAutomations(
      state.automations.map((item) => (item.id === id ? { ...item, group: trimmedGroup, updatedAt: nowIso() } : item))
    );
    state.notice = trimmedGroup
      ? `Moved "${automation.name}" to "${trimmedGroup}".`
      : `Removed "${automation.name}" from its group.`;
  } catch (assignError) {
    state.error = String(assignError);
  }
  render();
}
