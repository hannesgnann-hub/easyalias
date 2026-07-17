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
// commandPreview is stored too, so the backend can write aliases.zsh without
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
  aliasesFile: string;
  sourceLine: string;
  zshrcSourcePresent: boolean;
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

// Suggestions use the same fields as the create form and add display metadata.
// Keeping them structured means previews and validation use the normal app logic.
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

// Conservative macOS defaults that are useful without modifying or deleting data.
// A suggestion only pre-fills the form; the user still reviews and saves it.
const aliasSuggestions: AliasSuggestion[] = [
  {
    id: "list-details",
    name: "ll",
    path: "",
    action: "custom",
    customCommand: "ls -lah",
    description: "Detailed file list"
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
    id: "list-ports",
    name: "ports",
    path: "",
    action: "custom",
    customCommand: "lsof -nP -iTCP -sTCP:LISTEN",
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
    id: "open-finder",
    name: "finder",
    path: "~",
    action: "open",
    customCommand: "",
    description: "Open your home folder"
  },
  {
    id: "reload-zsh",
    name: "reloadzsh",
    path: "",
    action: "custom",
    customCommand: "source ~/.zshrc",
    description: "Reload zsh configuration"
  }
];

// Global UI state. For this prototype we keep state in module-level variables
// and re-render the app when larger UI structure changes.
let appState: AppState = {
  aliases: [],
  configFile: "~/.easyalias/config.json",
  aliasesFile: "~/.easyalias/aliases.zsh",
  sourceLine: "source ~/.easyalias/aliases.zsh",
  zshrcSourcePresent: false
};

let form: AliasForm = { ...emptyForm };
let editForm: AliasForm | null = null;
let editingId: string | null = null;
// Suggestions start collapsed so they do not compete with the main workflow.
// The state remains stable across normal renders until the user toggles it.
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

// Opens the native macOS file/folder picker through Tauri.
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

// Footer links need Tauri's opener plugin in the desktop app.
// A normal target="_blank" link is fine in browser preview, but not reliable in WebView.
async function openRepository(event: Event) {
  event.preventDefault();

  if (!isTauriRuntime()) {
    window.open(repoUrl, "_blank", "noopener,noreferrer");
    return;
  }

  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(repoUrl);
  } catch (openError) {
    error = `GitHub could not be opened: ${String(openError)}`;
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

// Converts a user-entered path into a safe zsh command argument.
// "~/" is expanded to "$HOME/" so generated aliases keep working reliably.
function shellPath(path: string) {
  const trimmed = path.trim();
  if (!trimmed) return "";

  if (trimmed === "~") return '"$HOME"';
  if (trimmed.startsWith("~/")) {
    return `"$HOME/${escapeDoubleQuoted(trimmed.slice(2))}"`;
  }

  return `"${escapeDoubleQuoted(trimmed)}"`;
}

// Escape characters that can break a double-quoted zsh string.
function escapeDoubleQuoted(value: string) {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/`/g, "\\`").replace(/\$/g, "\\$");
}

// Converts the selected action + path/custom command into the shell command
// that will later be written into aliases.zsh.
function buildCommandPreview(entry: Pick<AliasEntry, "path" | "action" | "customCommand">) {
  const path = shellPath(entry.path);

  switch (entry.action) {
    case "navigate":
      return path ? `cd ${path}` : "";
    case "open":
      return path ? `open ${path}` : "";
    case "execute":
      return path;
    case "compile_gradle":
      return path ? `cd ${path} && ./gradlew build` : "";
    case "compile_maven":
      return path ? `cd ${path} && mvn clean package` : "";
    case "custom":
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
    appState = JSON.parse(saved) as AppState;
  }

  render();
}

// Persists current aliases. Tauri writes real files; browser preview only writes localStorage.
async function saveState() {
  clearMessages();

  const aliases = [...appState.aliases].sort((a, b) => a.name.localeCompare(b.name));

  if (isTauriRuntime()) {
    try {
      appState = await invokeCommand<AppState>("save_aliases", { aliases });
      notice = `Saved: ${appState.aliasesFile}`;
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

// Copy a suggestion into the regular create form. Nothing is persisted until
// the user presses Add, so every suggested name and command remains editable.
function useSuggestion(id: string) {
  const suggestion = aliasSuggestions.find((item) => item.id === id);
  if (!suggestion) return;

  form = {
    name: suggestion.name,
    path: suggestion.path,
    action: suggestion.action,
    customCommand: suggestion.customCommand
  };
  clearMessages();
  render();

  // Select the proposed name so it is easy to replace without extra clicks.
  requestAnimationFrame(() => {
    const nameInput = document.querySelector<HTMLInputElement>('input[name="name"]');
    nameInput?.focus();
    nameInput?.select();
  });
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
          <p class="eyebrow">macOS Alias Manager</p>
          <h1>EasyAlias</h1>
        </div>
        <button class="ghost-button" data-action="reset">New</button>
      </header>

      <section class="status-grid">
        <div>
          <span>Alias File</span>
          <strong>${appState.aliasesFile}</strong>
        </div>
        <div>
          <span>.zshrc Source</span>
          <strong>${appState.zshrcSourcePresent ? "Connected" : "Not connected yet"}</strong>
        </div>
        <div>
          <span>Aliases</span>
          <strong>${aliases.length}</strong>
        </div>
      </section>

      ${
        appState.zshrcSourcePresent
          ? ""
          : `<aside class="source-hint">
              <span>Automatically added to ~/.zshrc on first Tauri startup:</span>
              <code>${appState.sourceLine}</code>
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
                  <textarea name="customCommand" rows="4" placeholder='cd "$HOME/project" && ./run.sh'>${escapeHtml(form.customCommand)}</textarea>
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
        <a href="${repoUrl}" target="_blank" rel="noreferrer" data-action="open-repo">
          © Hannes Gnann
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
                <textarea name="edit-customCommand" rows="4" placeholder='cd "$HOME/project" && ./run.sh'>${escapeHtml(editForm.customCommand)}</textarea>
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
  document.querySelector<HTMLAnchorElement>('[data-action="open-repo"]')?.addEventListener("click", openRepository);

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
        if (suggestionId) useSuggestion(suggestionId);
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
