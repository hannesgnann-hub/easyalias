// Running an automation step by step.

import { escapeHtml } from "../html";
import { createId, invokeCommand, isTauriRuntime } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { AutomationCommandResult, AutomationRunState } from "../types";
import { automationStepLabel } from "./editor";

export async function stopAutomation() {
  if (!state.automationRun?.running || state.automationRun.cancelRequested) return;
  state.automationRun.cancelRequested = true;
  render();
  // Killing the session immediately interrupts a command that is still
  // running; without this, Stop would only take effect once that command
  // finished on its own and the loop reached its next cancellation check.
  try {
    await invokeCommand<void>("stop_automation_session", { sessionId: state.automationRun.sessionId });
  } catch {
    // The run loop's own cleanup will report the failure if the session is gone.
  }
}

export function closeAutomationRun() {
  if (state.automationRun?.running) return;
  state.automationRun = null;
  render();
}

export async function waitForAutomation(seconds: number, state: AutomationRunState) {
  const end = Date.now() + seconds * 1000;
  while (Date.now() < end) {
    if (state.cancelRequested) return false;
    await new Promise((resolve) => setTimeout(resolve, Math.min(250, end - Date.now())));
  }
  return !state.cancelRequested;
}

export async function runAutomation(id: string) {
  if (state.automationRun?.running || state.automationBusy) return;
  const automation = state.automations.find((item) => item.id === id);
  if (!automation) return;
  if (!isTauriRuntime()) {
    state.error = "Automations can only run inside the EasyAlias desktop app.";
    render();
    return;
  }

  const runState: AutomationRunState = {
    automationId: id,
    sessionId: createId(),
    running: true,
    cancelRequested: false,
    currentStep: 0,
    error: "",
    steps: automation.steps.map(() => ({ status: "pending", output: "" }))
  };
  state.automationRun = runState;
  render();

  // All command steps share this one shell session, so `cd` and exported
  // variables from an earlier step are still in effect for later ones -
  // the whole run behaves like one continuous terminal, not isolated calls.
  try {
    await invokeCommand<void>("start_automation_session", { sessionId: runState.sessionId, path: automation.path });
  } catch (sessionError) {
    runState.error = `Automation session could not be started: ${String(sessionError)}`;
    runState.steps = runState.steps.map((step) => ({ ...step, status: "skipped" }));
    runState.running = false;
    render();
    return;
  }

  for (const [index, step] of automation.steps.entries()) {
    if (runState.cancelRequested) break;
    runState.currentStep = index;
    runState.steps[index].status = "running";
    render();

    try {
      if (step.kind === "wait") {
        const completed = await waitForAutomation(step.seconds, runState);
        if (!completed) break;
        runState.steps[index] = {
          status: "success",
          output: `Waited ${step.seconds} ${step.seconds === 1 ? "second" : "seconds"}.`
        };
      } else {
        const result = await invokeCommand<AutomationCommandResult>("run_session_command", {
          sessionId: runState.sessionId,
          command: step.command,
          background: step.behavior === "background"
        });
        const output = [result.stdout.trim(), result.stderr.trim()].filter(Boolean).join("\n");
        if (step.behavior === "background") {
          runState.steps[index] = {
            status: "success",
            output: result.processId ? `Started in background (PID ${result.processId}).` : "Started in background."
          };
        } else if (result.exitCode === 0) {
          runState.steps[index] = { status: "success", output: output || "Command completed." };
        } else {
          runState.steps[index] = {
            status: "error",
            output: output || `Command exited with code ${result.exitCode ?? "unknown"}.`
          };
          runState.error = `Step ${index + 1} failed. Remaining steps were not started.`;
          break;
        }
      }
    } catch (runError) {
      runState.steps[index] = { status: "error", output: String(runError) };
      runState.error = `Step ${index + 1} could not be completed.`;
      break;
    }
  }

  if (runState.cancelRequested) {
    runState.error = "Automation stopped. A background process that already started keeps running.";
  }
  try {
    await invokeCommand<void>("stop_automation_session", { sessionId: runState.sessionId });
  } catch {
    // The session's own process already exited; nothing left to clean up.
  }
  runState.steps = runState.steps.map((step) =>
    step.status === "pending" ? { ...step, status: "skipped" } : step
  );
  runState.running = false;
  render();
}

export function renderAutomationRun() {
  if (!state.automationRun) return "";
  const automation = state.automations.find((item) => item.id === state.automationRun?.automationId);
  if (!automation) return "";
  const successful = state.automationRun.steps.filter((step) => step.status === "success").length;

  return `
    <section class="modal-layer" role="presentation">
      <section class="modal-card automation-runner" role="dialog" aria-modal="true" aria-labelledby="automation-run-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow automation-run-state">${state.automationRun.running ? '<i class="automation-spinner" data-lucide="loader-circle"></i><span>Running...</span>' : state.automationRun.error ? "Run stopped" : "Completed"}</p>
            <h2 id="automation-run-title">${escapeHtml(automation.name)}</h2>
          </div>
          <span class="automation-progress">${successful} / ${automation.steps.length}</span>
        </div>
        <p class="automation-run-path"><i data-lucide="folder-open"></i><code>${escapeHtml(automation.path)}</code></p>
        ${
          state.automationRun.running
            ? `<div class="automation-running-banner" role="status">
                <span>Step ${state.automationRun.currentStep + 1} is running. EasyAlias is waiting for it to finish.</span>
                <span class="automation-running-track" aria-hidden="true"><span></span></span>
              </div>`
            : ""
        }
        ${state.automationRun.error ? `<p class="modal-error">${escapeHtml(state.automationRun.error)}</p>` : ""}
        <div class="automation-run-list">
          ${automation.steps
            .map((step, index) => {
              const result = state.automationRun!.steps[index];
              return `<article class="automation-run-step is-${result.status}">
                <div class="run-step-marker">${index + 1}</div>
                <div class="run-step-copy">
                  <div><strong>${step.kind === "wait" ? "Wait" : "Command"}</strong><span>${result.status === "running" ? '<i class="automation-spinner" data-lucide="loader-circle"></i>Running...' : result.status}</span></div>
                  <code>${escapeHtml(automationStepLabel(step))}</code>
                  ${result.output ? `<pre>${escapeHtml(result.output)}</pre>` : ""}
                </div>
              </article>`;
            })
            .join("")}
        </div>
        <div class="modal-actions">
          ${
            state.automationRun.running
              ? `<button class="danger-button" type="button" data-automation-action="stop-run" ${state.automationRun.cancelRequested ? "disabled" : ""}><i data-lucide="circle-stop"></i><span>${state.automationRun.cancelRequested ? "Stopping..." : "Stop"}</span></button>`
              : `<button class="ghost-button" type="button" data-automation-action="close-run">Close</button>
                 <button class="primary-button" type="button" data-automation-action="run" data-id="${escapeHtml(automation.id)}"><i data-lucide="play"></i><span>Run again</span></button>`
          }
        </div>
      </section>
    </section>`;
}
