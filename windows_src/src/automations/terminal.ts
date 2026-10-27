import { invokeCommand, isTauriRuntime } from "../platform";
import { render } from "../render";
import { state } from "../state";

export function toggleTerminalPicker(id: string) {
  state.terminalPickerAutomationId = state.terminalPickerAutomationId === id ? null : id;
  state.automationGroupPickerId = null; state.scheduleEditorAutomationId = null; state.hotkeyEditorAutomationId = null; render();
}
export async function openAutomationTerminal(automationId: string, terminalId: string) {
  const automation = state.automations.find((item) => item.id === automationId); const terminal = state.terminalApps.find((item) => item.id === terminalId);
  if (!automation || !terminal || state.terminalBusy) return;
  state.terminalBusy = true;
  try { if (!isTauriRuntime()) throw new Error("Opening terminals is available in the desktop app."); await invokeCommand<void>("open_in_terminal", { terminalId, path: automation.path }); state.notice = `${terminal.name} opened in ${automation.path}.`; state.error = ""; state.terminalPickerAutomationId = null; }
  catch (error) { state.error = String(error); }
  finally { state.terminalBusy = false; render(); }
}
