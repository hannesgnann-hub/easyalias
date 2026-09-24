// Choosing the home folder the sandboxed app may access.

import { clearMessages } from "./messages";
import { invokeCommand, isTauriRuntime } from "./platform";
import { render } from "./render";
import { state } from "./state";
import type { AppState } from "./types";

// App Sandbox access starts with an explicit Home-folder selection. The Rust
// backend turns that temporary permission into a security-scoped bookmark and
// only uses it for .zshrc, .bash_profile, and .bashrc.
export async function chooseHomeFolder() {
  clearMessages();
  state.importError = "";

  if (!isTauriRuntime()) {
    state.error = "The Home folder connection is only available in the Tauri app.";
    render();
    return;
  }

  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: false,
      directory: true,
      title: "Choose your Home folder"
    });

    if (typeof selected !== "string") return;

    state.appState = await invokeCommand<AppState>("connect_home", { path: selected });
    state.selectedImportIds = new Set(state.appState.importCandidates.map((candidate) => candidate.id));
    state.manualImportOpen = state.appState.importCandidates.length > 0;
    state.notice = state.manualImportOpen
      ? "Connected. Review the existing aliases found in your shell files."
      : "Home folder connected. zsh and Bash startup files are ready.";
  } catch (connectionFailure) {
    state.error = String(connectionFailure);
  }

  render();
}

export async function changeHomeFolder() {
  if (!state.appState.homeConnected) {
    await chooseHomeFolder();
    return;
  }

  const confirmed = window.confirm(
    "Change the connected Home folder? EasyAlias will back up .zshrc, .bash_profile, and .bashrc and remove its managed blocks first."
  );
  if (!confirmed) return;

  clearMessages();
  try {
    state.appState = await invokeCommand<AppState>("disconnect_home");
    state.notice = "Previous Home folder disconnected.";
    render();
    await chooseHomeFolder();
  } catch (disconnectFailure) {
    state.error = String(disconnectFailure);
    render();
  }
}
