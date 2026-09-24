import "./styles.css";
import { inspectBackupFile } from "./aliases/backup";
import { inspectAutomationBackupFile } from "./automations/backup";
import { runAutomation } from "./automations/run";
import { renderAutomationsView } from "./automations/view";
import { closeAllAutomationCardPopovers } from "./navigation";
import { loadState } from "./persistence";
import { isTauriRuntime } from "./platform";
import { applyTheme } from "./preferences";
import { render } from "./render";
import { state } from "./state";

// Tauri reports native file drops even though the HTML drop event does not
// contain a browser File object. Only drops while the backup dialog is open are
// consumed; dropping multiple files produces a clear validation message.
async function bindNativeBackupDrop() {
  if (!isTauriRuntime()) return;

  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    await getCurrentWebview().onDragDropEvent((event) => {
      const isAliasImport = state.backupDialogMode === "import";
      const isAutomationImport = state.automationBackupDialogMode === "import";
      if (!isAliasImport && !isAutomationImport) return;

      const dropZoneSelector = isAutomationImport
        ? ".automation-backup-drop-zone"
        : ".backup-drop-zone";

      if (event.payload.type === "enter" || event.payload.type === "over") {
        document.querySelector(dropZoneSelector)?.classList.add("is-dragging");
        return;
      }

      document.querySelector(dropZoneSelector)?.classList.remove("is-dragging");
      if (event.payload.type !== "drop") return;
      if (event.payload.paths.length !== 1) {
        if (isAutomationImport) {
          state.automationBackupError = "Drop exactly one EasyAlias automation JSON backup.";
          renderAutomationsView();
        } else {
          state.backupError = "Drop exactly one EasyAlias JSON backup.";
          render();
        }
        return;
      }

      if (isAutomationImport) {
        void inspectAutomationBackupFile(event.payload.paths[0]);
      } else {
        void inspectBackupFile(event.payload.paths[0]);
      }
    });
  } catch (dropError) {
    console.warn("Native backup drop could not be initialized", dropError);
  }
}

// Follow the OS light/dark setting live while the theme preference is "system".
if (typeof window.matchMedia === "function") {
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (state.appSettings.theme === "system") applyTheme("system");
  });
}

// Global automation hotkeys are registered natively; the backend tells the
// frontend when one fires so the run can surface here (or a toast can show a
// background run's result).
async function bindAutomationHotkeyEvents() {
  if (!isTauriRuntime()) return;
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<string>("automation-hotkey-fired", (event) => {
      const automationId = event.payload;
      if (!state.automations.some((automation) => automation.id === automationId)) return;
      state.currentView = "automations";
      closeAllAutomationCardPopovers();
      render();
      void runAutomation(automationId);
    });
    await listen<{ name: string; ok: boolean; message: string }>(
      "automation-hotkey-result",
      (event) => {
        const { name, ok, message } = event.payload;
        if (ok) {
          state.notice = `"${name}" ran from its shortcut.`;
          state.error = "";
        } else {
          state.error = `"${name}" failed: ${message}`;
          state.currentView = "automations";
        }
        render();
      }
    );
  } catch (hotkeyEventError) {
    console.warn("Automation hotkey events could not be initialized", hotkeyEventError);
  }
}

// Initial app boot.
void bindNativeBackupDrop();

void bindAutomationHotkeyEvents();

void loadState();
