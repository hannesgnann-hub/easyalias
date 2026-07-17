import "./styles.css";

// Actions are the high-level choices shown in the dropdown.
// The selected action decides how the final shell command is generated.
type AliasAction =
  | "navigate"
  | "open"
  | "execute"
  | "compile_gradle"
  | "compile_maven"
  | "custom";

// This is the canonical alias data shape used by the UI and persisted as JSON.
// commandPreview is stored too, so the backend can write .cmd files without
// needing to duplicate all frontend command-generation rules.
type AliasEntry = {
  id: string;
  name: string;
  path: string;
  action: AliasAction;
  customCommand?: string;
  commandPreview: string;
  createdAt: string;
  updatedAt: string;
};

// AppState mirrors what the Rust backend returns to the frontend.
// The file paths are included so the UI can show where EasyAlias stores data.
type AppState = {
  aliases: AliasEntry[];
  configFile: string;
  // Folder that contains generated command files such as test1.cmd.
  // This is the important Windows integration point: cmd.exe discovers aliases
  // because this folder is added to the user's PATH by the Rust backend.
  commandDir: string;
  // Absolute PATH entry shown in the UI when the user needs to restart Terminal.
  pathEntry: string;
  // True when the backend can see commandDir in the persisted User PATH or in
  // the current process PATH. A freshly updated PATH usually needs a new shell.
  pathConfigured: boolean;
};

// AliasForm is the temporary state for either the create form or the edit modal.
// It is intentionally close to AliasEntry but does not include timestamps.
type AliasForm = {
  id?: string;
  name: string;
  path: string;
  action: AliasAction;
  customCommand: string;
};

// Suggestions share the regular form fields so command generation and direct
// persistence follow exactly the same path as manually created shortcuts.
type AliasSuggestion = AliasForm & {
  id: string;
  description: string;
};

type PickerTarget = "create" | "edit";
type PickerKind = "file" | "folder";

const actionLabels: Record<AliasAction, string> = {
  navigate: "Go to Folder",
  open: "Open",
  execute: "Run",
  compile_gradle: "Gradle Build",
  compile_maven: "Maven Build",
  custom: "Custom Command"
};

const emptyForm: AliasForm = {
  name: "",
  path: "",
  action: "navigate",
  customCommand: ""
};

// Windows/cmd.exe starter shortcuts. Commands that wrap another batch file use
// `call`; generic wrappers such as gw also forward all user arguments with %*.
const aliasSuggestions: AliasSuggestion[] = [
  {
    id: "list-details",
    name: "ll",
    path: "",
    action: "custom",
    customCommand: "dir /a",
    description: "Detailed file list"
  },
  {
    id: "clear-terminal",
    name: "c",
    path: "",
    action: "custom",
    customCommand: "cls",
    description: "Clear the terminal"
  },
  {
    id: "git-status",
    name: "gs",
    path: "",
    action: "custom",
    customCommand: "git status --short --branch",
    description: "Compact Git status"
  },
  {
    id: "gradle-wrapper",
    name: "gw",
    path: "",
    action: "custom",
    customCommand: "call gradlew.bat %*",
    description: "Run the Gradle wrapper"
  },
  {
    id: "gradle-wrapper-build",
    name: "gwb",
    path: "",
    action: "custom",
    customCommand: "call gradlew.bat build",
    description: "Build with Gradle wrapper"
  },
  {
    id: "gradle-wrapper-test",
    name: "gwtest",
    path: "",
    action: "custom",
    customCommand: "call gradlew.bat test",
    description: "Run Gradle tests"
  },
  {
    id: "maven-wrapper",
    name: "mw",
    path: "",
    action: "custom",
    customCommand: "call mvnw.cmd %*",
    description: "Run the Maven wrapper"
  },
  {
    id: "git-log-graph",
    name: "glog",
    path: "",
    action: "custom",
    customCommand: "git log --oneline --graph --decorate --all",
    description: "Compact Git history graph"
  },
  {
    id: "python-server",
    name: "serve",
    path: "",
    action: "custom",
    customCommand: "python -m http.server",
    description: "Serve the current folder"
  },
  {
    id: "docker-compose-up",
    name: "dcu",
    path: "",
    action: "custom",
    customCommand: "docker compose up -d",
    description: "Start Docker Compose"
  },
  {
    id: "list-ports",
    name: "ports",
    path: "",
    action: "custom",
    customCommand: "netstat -ano | findstr LISTENING",
    description: "Show listening TCP ports"
  },
  {
    id: "downloads-folder",
    name: "downloads",
    path: "~/Downloads",
    action: "navigate",
    customCommand: "",
    description: "Jump to Downloads"
  },
  {
    id: "open-home",
    name: "home",
    path: "~",
    action: "open",
    customCommand: "",
    description: "Open your home folder"
  },
  {
    id: "network-info",
    name: "netinfo",
    path: "",
    action: "custom",
    customCommand: "ipconfig",
    description: "Show network configuration"
  }
];

// Global UI state. For this prototype we keep state in module-level variables
// and re-render the app when larger UI structure changes.
let appState: AppState = {
  aliases: [],
  configFile: "~/.easyalias/config.json",
  commandDir: "~/.easyalias/bin",
  pathEntry: "~/.easyalias/bin",
  pathConfigured: false
};

let form: AliasForm = { ...emptyForm };
let editForm: AliasForm | null = null;
let editingId: string | null = null;
// Suggestions remain out of the main workflow until the user expands them.
let suggestionsExpanded = false;
let notice = "";
let error = "";
let editError = "";

// Vite mounts the app into <main id="app"> from index.html.
const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
  throw new Error("App container not found");
}

const appElement = app;
const repoUrl = "https://github.com/hannesgnann-hub/easyalias";
const redditUrl = "https://www.reddit.com/r/easyalias/";

// Tauri injects this marker only inside the native desktop runtime.
// Browser preview mode uses localStorage and skips native-only features.
function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

// Small wrapper around Tauri's invoke API, keeping the rest of the code typed.
async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

// Opens the native file/folder picker through Tauri.
// In browser preview mode, there is no native dialog, so we show a friendly message.
async function openPathPicker(target: PickerTarget, kind: PickerKind) {
  clearMessages();
  editError = "";

  if (!isTauriRuntime()) {
    error = "The file/folder picker only works in the Tauri app, not in browser preview.";
    render();
    return;
  }

  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: false,
      directory: kind === "folder"
    });

    if (typeof selected !== "string") return;

    if (target === "create") {
      updateForm("path", selected);
      const input = document.querySelector<HTMLInputElement>('input[name="path"]');
      if (input) input.value = selected;
      return;
    }

    updateEditForm("path", selected);
    const input = document.querySelector<HTMLInputElement>('input[name="edit-path"]');
    if (input) input.value = selected;
  } catch (pickerError) {
    const message = `Picker could not be opened: ${String(pickerError)}`;
    if (target === "edit") {
      editError = message;
    } else {
      error = message;
    }
    render();
  }
}

// Static footer links share the opener plugin so both GitHub and Reddit open in
// the user's system browser instead of inside the Tauri WebView.
async function openExternalLink(event: Event) {
  event.preventDefault();
  const anchor = event.currentTarget as HTMLAnchorElement;
  const targetUrl = anchor.href;

  if (!isTauriRuntime()) {
    window.open(targetUrl, "_blank", "noopener,noreferrer");
    return;
  }

  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(targetUrl);
  } catch (openError) {
    error = `Link could not be opened: ${String(openError)}`;
    render();
  }
}

// Prefer a browser UUID. The fallback only exists for older WebViews.
function createId() {
  if ("crypto" in window && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }

  return `alias_${Date.now()}_${Math.random().toString(16).slice(2)}`;
}

// Store timestamps as ISO strings because they are easy to persist and format later.
function nowIso() {
  return new Date().toISOString();
}

// Converts a user-entered path into a safe cmd.exe command argument.
//
// Examples:
//   ~/Desktop/app      -> "%USERPROFILE%\Desktop\app"
//   C:\Tools\run.bat   -> "C:\Tools\run.bat"
//
// The generated command is written into a .cmd file. That means we intentionally
// use cmd.exe syntax here, not PowerShell syntax.
function cmdPath(path: string) {
  const trimmed = path.trim();
  if (!trimmed) return "";

  if (trimmed === "~") return '"%USERPROFILE%"';
  if (trimmed.startsWith("~/") || trimmed.startsWith("~\\")) {
    const withoutHome = trimmed.slice(2).replace(/\//g, "\\");
    return `"%USERPROFILE%\\${escapeCmdDoubleQuoted(withoutHome)}"`;
  }

  return `"${escapeCmdDoubleQuoted(trimmed)}"`;
}

// Escape characters that can break a double-quoted batch string.
//
// Percent signs are special in .cmd files because %NAME% means environment
// variable expansion. Doubling percent signs keeps literal percent signs intact.
// Double quotes are doubled so the generated string remains a single argument.
function escapeCmdDoubleQuoted(value: string) {
  return value.replace(/%/g, "%%").replace(/"/g, '""');
}

// Converts the selected action + path/custom command into the shell command
// that will later be written into a .cmd file.
function buildCommandPreview(entry: Pick<AliasEntry, "path" | "action" | "customCommand">) {
  const path = cmdPath(entry.path);

  switch (entry.action) {
    case "navigate":
      // /d is important: it allows changing drives, e.g. C: -> D:.
      return path ? `cd /d ${path}` : "";
    case "open":
      // The empty title argument is required by start when the target is quoted.
      return path ? `start "" ${path}` : "";
    case "execute":
      // call preserves batch behavior and forwards any extra CLI arguments.
      return path ? `call ${path} %*` : "";
    case "compile_gradle":
      // Run from the selected project folder so gradlew.bat resolves locally.
      return path ? `cd /d ${path} && call gradlew.bat build` : "";
    case "compile_maven":
      // Maven is expected on PATH; the selected folder becomes the build root.
      return path ? `cd /d ${path} && call mvn clean package` : "";
    case "custom":
      // Custom commands are passed through deliberately. The user owns them.
      return entry.customCommand?.trim() ?? "";
  }
}

// Shared validation for create and edit forms.
// Alias names are intentionally conservative because they become shell identifiers.
function validateAlias(formValue: AliasForm) {
  if (!/^[A-Za-z_][A-Za-z0-9_-]*$/.test(formValue.name.trim())) {
    return "Alias name must start with a letter or _ and may only contain letters, numbers, _ or -.";
  }

  if (formValue.action === "custom") {
    if (!formValue.customCommand.trim()) return "Custom Command cannot be empty.";
    return "";
  }

  if (!formValue.path.trim()) return "Please enter a path or command.";

  return "";
}

// Loads aliases from the Rust backend in Tauri, or from localStorage in browser preview.
async function loadState() {
  clearMessages();

  if (isTauriRuntime()) {
    try {
      appState = await invokeCommand<AppState>("load_aliases");
      render();
      return;
    } catch (loadError) {
      error = String(loadError);
    }
  }

  const saved = localStorage.getItem("easyalias-state");
  if (saved) {
    // Merge instead of replacing so older browser-preview state from the earlier
    // PowerShell version does not drop the newer commandDir/path fields.
    appState = { ...appState, ...(JSON.parse(saved) as Partial<AppState>) };
  }

  render();
}

// Persists current aliases. Tauri writes real files; browser preview only writes localStorage.
async function saveState() {
  clearMessages();

  const aliases = [...appState.aliases].sort((a, b) => a.name.localeCompare(b.name));

  if (isTauriRuntime()) {
    try {
      // The backend is authoritative for files and PATH status. It may also
      // normalize old commandPreview values into the current cmd.exe format.
      appState = await invokeCommand<AppState>("save_aliases", { aliases });
      notice = `Saved: ${appState.commandDir}`;
      render();
      return;
    } catch (saveError) {
      error = String(saveError);
      render();
      return;
    }
  }

  appState = { ...appState, aliases };
  localStorage.setItem("easyalias-state", JSON.stringify(appState));
  notice = "Browser preview saved. In Tauri, the app writes real files.";
  render();
}

// Message helpers keep the visible notice/error state separate from form data.
function clearMessages() {
  notice = "";
  error = "";
}

function clearRenderedMessages() {
  document.querySelector(".notice")?.remove();
  document.querySelector(".error")?.remove();
}

function resetForm() {
  form = { ...emptyForm };
  clearMessages();
  render();
}

function toggleSuggestions() {
  suggestionsExpanded = !suggestionsExpanded;
  render();
}

// Turn one suggestion into a real AliasEntry immediately. The duplicate guard
// also protects against a second click that arrives while a save is in flight.
async function useSuggestion(id: string) {
  const suggestion = aliasSuggestions.find((item) => item.id === id);
  if (!suggestion) return;

  if (appState.aliases.some((alias) => alias.name === suggestion.name)) {
    error = `Alias "${suggestion.name}" already exists.`;
    render();
    return;
  }

  const timestamp = nowIso();
  const nextAlias: AliasEntry = {
    id: createId(),
    name: suggestion.name,
    path: suggestion.path,
    action: suggestion.action,
    customCommand: suggestion.action === "custom" ? suggestion.customCommand : undefined,
    commandPreview: buildCommandPreview(suggestion),
    createdAt: timestamp,
    updatedAt: timestamp
  };

  appState = {
    ...appState,
    aliases: [...appState.aliases, nextAlias]
  };
  clearMessages();
  await saveState();
}

// Opens the edit modal by copying the persisted alias into temporary editForm state.
// Changes are not saved until the modal form is submitted.
function openEditModal(id: string) {
  const alias = appState.aliases.find((item) => item.id === id);
  if (!alias) return;

  editingId = id;
  editForm = {
    id: alias.id,
    name: alias.name,
    path: alias.path,
    action: alias.action,
    customCommand: alias.customCommand ?? ""
  };
  editError = "";
  clearMessages();
  render();
}

function closeEditModal() {
  editingId = null;
  editForm = null;
  editError = "";
  render();
}

async function upsertAlias(event: SubmitEvent) {
  event.preventDefault();
  clearMessages();

  const validationError = validateAlias(form);
  if (validationError) {
    error = validationError;
    render();
    return;
  }

  const duplicate = appState.aliases.find(
    (alias) => alias.name === form.name.trim()
  );

  if (duplicate) {
    error = `Alias "${form.name.trim()}" already exists.`;
    render();
    return;
  }

  const timestamp = nowIso();
  const nextAlias: AliasEntry = {
    id: createId(),
    name: form.name.trim(),
    path: form.path.trim(),
    action: form.action,
    customCommand: form.action === "custom" ? form.customCommand.trim() : undefined,
    commandPreview: buildCommandPreview(form),
    createdAt: timestamp,
    updatedAt: timestamp
  };

  appState = {
    ...appState,
    aliases: [...appState.aliases, nextAlias]
  };

  form = { ...emptyForm };
  await saveState();
}

// Saves edits from the modal while preserving the original id and createdAt timestamp.
async function updateAlias(event: SubmitEvent) {
  event.preventDefault();
  if (!editForm || !editingId) return;

  editError = validateAlias(editForm);
  if (editError) {
    render();
    return;
  }

  const duplicate = appState.aliases.find(
    (alias) => alias.name === editForm?.name.trim() && alias.id !== editingId
  );

  if (duplicate) {
    editError = `Alias "${editForm.name.trim()}" already exists.`;
    render();
    return;
  }

  const existing = appState.aliases.find((alias) => alias.id === editingId);
  if (!existing) {
    closeEditModal();
    return;
  }

  const nextAlias: AliasEntry = {
    id: existing.id,
    name: editForm.name.trim(),
    path: editForm.path.trim(),
    action: editForm.action,
    customCommand: editForm.action === "custom" ? editForm.customCommand.trim() : undefined,
    commandPreview: buildCommandPreview(editForm),
    createdAt: existing.createdAt,
    updatedAt: nowIso()
  };

  appState = {
    ...appState,
    aliases: appState.aliases.map((alias) => (alias.id === existing.id ? nextAlias : alias))
  };

  editingId = null;
  editForm = null;
  editError = "";
  await saveState();
}

// Removes an alias and then rewrites the generated alias file through saveState().
async function deleteAlias(id: string) {
  appState = {
    ...appState,
    aliases: appState.aliases.filter((alias) => alias.id !== id)
  };

  if (editingId === id) {
    editingId = null;
    editForm = null;
    editError = "";
  }

  await saveState();
}

// Updates the create form. Most text changes update only the command preview,
// avoiding a full re-render so input focus is not lost while typing.
function updateForm<K extends keyof AliasForm>(key: K, value: AliasForm[K], rerender = false) {
  form = { ...form, [key]: value };
  clearMessages();

  if (rerender) {
    render();
    return;
  }

  clearRenderedMessages();
  updatePreview();
}

// Same as updateForm(), but scoped to the edit modal.
function updateEditForm<K extends keyof AliasForm>(key: K, value: AliasForm[K], rerender = false) {
  if (!editForm) return;

  editForm = { ...editForm, [key]: value };
  editError = "";

  if (rerender) {
    render();
    return;
  }

  clearRenderedEditError();
  updateEditPreview();
}

// Centralized display formatting for timestamps shown in alias cards.
function formatDate(value: string) {
  return new Intl.DateTimeFormat("en-US", {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(new Date(value));
}

function formPreview() {
  return buildCommandPreview(form) || "No command generated yet";
}

function updatePreview() {
  const preview = document.querySelector<HTMLElement>(".preview code");
  if (preview) {
    preview.textContent = formPreview();
  }
}

function editPreview() {
  return editForm ? buildCommandPreview(editForm) || "No command generated yet" : "";
}

function updateEditPreview() {
  const preview = document.querySelector<HTMLElement>(".modal-preview code");
  if (preview) {
    preview.textContent = editPreview();
  }
}

function clearRenderedEditError() {
  document.querySelector(".modal-error")?.remove();
}

// Main render function. This replaces the app HTML from state and then calls bindEvents().
// For a larger app, this would be a good candidate to split into smaller render helpers.
function render() {
  const aliases = [...appState.aliases].sort((a, b) => a.name.localeCompare(b.name));
  const existingNames = new Set(aliases.map((alias) => alias.name));
  const availableSuggestions = aliasSuggestions.filter(
    (suggestion) => !existingNames.has(suggestion.name)
  );

  appElement.innerHTML = `
    <section class="shell">
      <header class="topbar">
        <div>
          <p class="eyebrow">Windows Alias Manager</p>
          <h1>EasyAlias</h1>
        </div>
        <button class="ghost-button" data-action="reset">New</button>
      </header>

      <section class="status-grid">
        <div>
          <span>Command Folder</span>
          <strong>${appState.commandDir}</strong>
        </div>
        <div>
          <span>User PATH</span>
          <strong>${appState.pathConfigured ? "Connected" : "Restart terminal"}</strong>
        </div>
        <div>
          <span>Aliases</span>
          <strong>${aliases.length}</strong>
        </div>
      </section>

      ${
        appState.pathConfigured
          ? ""
          : `<aside class="source-hint">
              <span>EasyAlias adds this folder to your User PATH. Open a new terminal after first setup:</span>
              <code>${appState.pathEntry}</code>
            </aside>`
      }

      ${notice ? `<p class="notice">${notice}</p>` : ""}
      ${error ? `<p class="error">${error}</p>` : ""}

      ${
        availableSuggestions.length
          ? `<section class="suggestions" data-expanded="${suggestionsExpanded}" aria-labelledby="suggestions-title">
              <div class="suggestions-header">
                <div class="suggestions-heading">
                  <h2 id="suggestions-title">Suggestions</h2>
                  <span>${availableSuggestions.length} available</span>
                </div>
                <button
                  class="suggestions-toggle"
                  type="button"
                  title="${suggestionsExpanded ? "Hide suggestions" : "Show suggestions"}"
                  aria-label="${suggestionsExpanded ? "Hide suggestions" : "Show suggestions"}"
                  aria-expanded="${suggestionsExpanded}"
                  aria-controls="suggestion-list"
                  data-action="toggle-suggestions"
                ><span aria-hidden="true">${suggestionsExpanded ? "⌄" : "›"}</span></button>
              </div>
              ${
                suggestionsExpanded
                  ? `<div class="suggestion-grid" id="suggestion-list">
                      ${availableSuggestions
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
            <input name="name" value="${escapeHtml(form.name)}" placeholder="beerv2" autocomplete="off" />
          </label>

          <label>
            Location / File / Command
            <span class="path-picker-row">
              <input name="path" value="${escapeHtml(form.path)}" placeholder="~/Desktop/projects/beerv2_app" autocomplete="off" />
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
                    `<option value="${value}" ${form.action === value ? "selected" : ""}>${label}</option>`
                )
                .join("")}
            </select>
          </label>

          ${
            form.action === "custom"
              ? `<label>
                  Custom Command
                  <textarea name="customCommand" rows="4" placeholder='cd /d "%USERPROFILE%\\project" && run.bat'>${escapeHtml(form.customCommand)}</textarea>
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
            <span>${aliases.length} entries</span>
          </div>

          ${
            aliases.length
              ? aliases
                  .map(
                    (alias) => `
                      <article class="alias-row ${alias.id === editingId ? "selected" : ""}">
                        <div class="row-main">
                          <span class="alias-name">${alias.name}</span>
                          <span class="alias-action">${actionLabels[alias.action]}</span>
                          <code>${escapeHtml(alias.commandPreview)}</code>
                          <span class="created">Created ${formatDate(alias.createdAt)}</span>
                        </div>
                        <button class="edit-button" title="Edit" data-action="edit" data-id="${alias.id}">Edit</button>
                        <button class="icon-button" title="Delete" data-action="delete" data-id="${alias.id}">×</button>
                      </article>
                    `
                  )
                  .join("")
              : `<div class="empty-state">
                  <strong>No aliases yet</strong>
                  <span>Create your first command on the left.</span>
                </div>`
          }
        </section>
      </section>

      ${renderEditModal()}

      <footer class="app-footer">
        <a href="${repoUrl}" target="_blank" rel="noreferrer" data-external-link>
          © Hannes Gnann
        </a>
        <span aria-hidden="true">-</span>
        <a href="${redditUrl}" target="_blank" rel="noreferrer" data-external-link>
          Reddit
        </a>
      </footer>
    </section>
  `;

  bindEvents();
}

// Renders the modal only when editForm/editingId are set.
// Returning an empty string keeps the main template simple.
function renderEditModal() {
  if (!editForm || !editingId) return "";

  return `
    <section class="modal-layer" role="presentation">
      <form class="modal-card" id="edit-form" role="dialog" aria-modal="true" aria-labelledby="edit-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">Edit Alias</p>
            <h2 id="edit-title">${escapeHtml(editForm.name || "Alias")}</h2>
          </div>
          <button class="ghost-button modal-close" type="button" data-action="close-edit">Close</button>
        </div>

        ${editError ? `<p class="modal-error">${escapeHtml(editError)}</p>` : ""}

        <label>
          Command Name
          <input name="edit-name" value="${escapeHtml(editForm.name)}" placeholder="beerv2" autocomplete="off" />
        </label>

        <label>
          Location / File / Command
          <span class="path-picker-row">
            <input name="edit-path" value="${escapeHtml(editForm.path)}" placeholder="~/Desktop/projects/beerv2_app" autocomplete="off" />
            <button class="picker-button" type="button" title="Choose file" data-action="pick-path" data-target="edit" data-kind="file">File</button>
            <button class="picker-button" type="button" title="Choose folder" data-action="pick-path" data-target="edit" data-kind="folder">Folder</button>
          </span>
        </label>

        <label>
          Action
          <select name="edit-action">
            ${Object.entries(actionLabels)
              .map(
                ([value, label]) =>
                  `<option value="${value}" ${editForm?.action === value ? "selected" : ""}>${label}</option>`
              )
              .join("")}
          </select>
        </label>

        ${
          editForm.action === "custom"
            ? `<label>
                Custom Command
                <textarea name="edit-customCommand" rows="4" placeholder='cd /d "%USERPROFILE%\\project" && run.bat'>${escapeHtml(editForm.customCommand)}</textarea>
              </label>`
            : ""
        }

        <div class="preview modal-preview">
          <span>Preview</span>
          <code>${escapeHtml(editPreview())}</code>
        </div>

        <div class="modal-actions">
          <button class="ghost-button" type="button" data-action="close-edit">Cancel</button>
          <button class="primary-button" type="submit">Save</button>
        </div>
      </form>
    </section>
  `;
}

// Because render() replaces the DOM, event listeners are reattached after every render.
// Small live-preview updates skip render(), so their listeners stay intact.
function bindEvents() {
  document.querySelector<HTMLFormElement>("#alias-form")?.addEventListener("submit", upsertAlias);
  document.querySelector<HTMLFormElement>("#edit-form")?.addEventListener("submit", updateAlias);
  document.querySelectorAll<HTMLAnchorElement>("[data-external-link]").forEach((link) => {
    link.addEventListener("click", openExternalLink);
  });

  document.querySelector<HTMLInputElement>('input[name="name"]')?.addEventListener("input", (event) => {
    updateForm("name", (event.target as HTMLInputElement).value);
  });

  document.querySelector<HTMLInputElement>('input[name="path"]')?.addEventListener("input", (event) => {
    updateForm("path", (event.target as HTMLInputElement).value);
  });

  document.querySelector<HTMLSelectElement>('select[name="action"]')?.addEventListener("change", (event) => {
    updateForm("action", (event.target as HTMLSelectElement).value as AliasAction, true);
  });

  document.querySelector<HTMLTextAreaElement>('textarea[name="customCommand"]')?.addEventListener("input", (event) => {
    updateForm("customCommand", (event.target as HTMLTextAreaElement).value);
  });

  document.querySelector<HTMLInputElement>('input[name="edit-name"]')?.addEventListener("input", (event) => {
    updateEditForm("name", (event.target as HTMLInputElement).value);
  });

  document.querySelector<HTMLInputElement>('input[name="edit-path"]')?.addEventListener("input", (event) => {
    updateEditForm("path", (event.target as HTMLInputElement).value);
  });

  document.querySelector<HTMLSelectElement>('select[name="edit-action"]')?.addEventListener("change", (event) => {
    updateEditForm("action", (event.target as HTMLSelectElement).value as AliasAction, true);
  });

  document.querySelector<HTMLTextAreaElement>('textarea[name="edit-customCommand"]')?.addEventListener("input", (event) => {
    updateEditForm("customCommand", (event.target as HTMLTextAreaElement).value);
  });

  document.querySelectorAll<HTMLButtonElement>("[data-action]").forEach((button) => {
    button.addEventListener("click", () => {
      const action = button.dataset.action;
      const id = button.dataset.id;

      if (action === "reset") resetForm();
      if (action === "edit" && id) openEditModal(id);
      if (action === "close-edit") closeEditModal();
      if (action === "toggle-suggestions") toggleSuggestions();
      if (action === "use-suggestion") {
        const suggestionId = button.dataset.suggestionId;
        if (suggestionId) void useSuggestion(suggestionId);
      }
      if (action === "pick-path") {
        const target = button.dataset.target;
        const kind = button.dataset.kind;
        if ((target === "create" || target === "edit") && (kind === "file" || kind === "folder")) {
          void openPathPicker(target, kind);
        }
      }
      if (action === "delete" && id) void deleteAlias(id);
    });
  });
}

// Escape user-controlled strings before inserting them into template-string HTML.
function escapeHtml(value: string) {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

// Initial app boot.
void loadState();
