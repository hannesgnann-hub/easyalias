import "./styles.css";
import { inspectBackupFile } from "./aliases/backup";
import { loadState } from "./persistence";
import { isTauriRuntime } from "./platform";
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
      if (state.backupDialogMode !== "import") return;

      if (event.payload.type === "enter" || event.payload.type === "over") {
        document.querySelector(".backup-drop-zone")?.classList.add("is-dragging");
        return;
      }

      document.querySelector(".backup-drop-zone")?.classList.remove("is-dragging");
      if (event.payload.type !== "drop") return;
      if (event.payload.paths.length !== 1) {
        state.backupError = "Drop exactly one EasyAlias JSON backup.";
        render();
        return;
      }

      void inspectBackupFile(event.payload.paths[0]);
    });
  } catch (dropError) {
    console.warn("Native backup drop could not be initialized", dropError);
  }
}

// Initial app boot.
void bindNativeBackupDrop();

void loadState();
