import "./styles.css";

type AliasAction =
  | "navigate"
  | "open"
  | "execute"
  | "compile_gradle"
  | "compile_maven"
  | "custom";

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

type AppState = {
  aliases: AliasEntry[];
  configFile: string;
  aliasesFile: string;
  sourceLine: string;
  zshrcSourcePresent: boolean;
};

type AliasForm = {
  id?: string;
  name: string;
  path: string;
  action: AliasAction;
  customCommand: string;
};

type PickerTarget = "create" | "edit";
type PickerKind = "file" | "folder";

const actionLabels: Record<AliasAction, string> = {
  navigate: "Navigiere zu Ordner",
  open: "Öffnen",
  execute: "Ausführen",
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
let notice = "";
let error = "";
let editError = "";

const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
  throw new Error("App container not found");
}

const appElement = app;
const repoUrl = "https://github.com/hannesgnann-hub/easyalias";

function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

async function openPathPicker(target: PickerTarget, kind: PickerKind) {
  clearMessages();
  editError = "";

  if (!isTauriRuntime()) {
    error = "Der Datei-/Ordner-Picker funktioniert nur in der Tauri-App, nicht in der Browser-Vorschau.";
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
    const message = `Picker konnte nicht geöffnet werden: ${String(pickerError)}`;
    if (target === "edit") {
      editError = message;
    } else {
      error = message;
    }
    render();
  }
}

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
    error = `GitHub konnte nicht geöffnet werden: ${String(openError)}`;
    render();
  }
}

function createId() {
  if ("crypto" in window && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }

  return `alias_${Date.now()}_${Math.random().toString(16).slice(2)}`;
}

function nowIso() {
  return new Date().toISOString();
}

function shellPath(path: string) {
  const trimmed = path.trim();
  if (!trimmed) return "";

  if (trimmed === "~") return '"$HOME"';
  if (trimmed.startsWith("~/")) {
    return `"$HOME/${escapeDoubleQuoted(trimmed.slice(2))}"`;
  }

  return `"${escapeDoubleQuoted(trimmed)}"`;
}

function escapeDoubleQuoted(value: string) {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/`/g, "\\`").replace(/\$/g, "\\$");
}

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

function validateAlias(formValue: AliasForm) {
  if (!/^[A-Za-z_][A-Za-z0-9_-]*$/.test(formValue.name.trim())) {
    return "Alias-Name muss mit Buchstabe oder _ starten und darf nur Buchstaben, Zahlen, _ oder - enthalten.";
  }

  if (formValue.action === "custom") {
    if (!formValue.customCommand.trim()) return "Custom Command darf nicht leer sein.";
    return "";
  }

  if (!formValue.path.trim()) return "Bitte einen Pfad oder Befehl eintragen.";

  return "";
}

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

async function saveState() {
  clearMessages();

  const aliases = [...appState.aliases].sort((a, b) => a.name.localeCompare(b.name));

  if (isTauriRuntime()) {
    try {
      appState = await invokeCommand<AppState>("save_aliases", { aliases });
      notice = `Gespeichert: ${appState.aliasesFile}`;
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
  notice = "Browser-Vorschau gespeichert. In Tauri schreibt die App echte Dateien.";
  render();
}

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
    error = `Alias "${form.name.trim()}" gibt es schon.`;
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
    editError = `Alias "${editForm.name.trim()}" gibt es schon.`;
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

function formatDate(value: string) {
  return new Intl.DateTimeFormat("de-DE", {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(new Date(value));
}

function formPreview() {
  return buildCommandPreview(form) || "Noch kein Befehl generiert";
}

function updatePreview() {
  const preview = document.querySelector<HTMLElement>(".preview code");
  if (preview) {
    preview.textContent = formPreview();
  }
}

function editPreview() {
  return editForm ? buildCommandPreview(editForm) || "Noch kein Befehl generiert" : "";
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

function render() {
  const aliases = [...appState.aliases].sort((a, b) => a.name.localeCompare(b.name));

  appElement.innerHTML = `
    <section class="shell">
      <header class="topbar">
        <div>
          <p class="eyebrow">macOS Alias Manager</p>
          <h1>EasyAlias</h1>
        </div>
        <button class="ghost-button" data-action="reset">Neu</button>
      </header>

      <section class="status-grid">
        <div>
          <span>Alias-Datei</span>
          <strong>${appState.aliasesFile}</strong>
        </div>
        <div>
          <span>.zshrc Source</span>
          <strong>${appState.zshrcSourcePresent ? "Verbunden" : "Noch einzutragen"}</strong>
        </div>
        <div>
          <span>Aliase</span>
          <strong>${aliases.length}</strong>
        </div>
      </section>

      ${
        appState.zshrcSourcePresent
          ? ""
          : `<aside class="source-hint">
              <span>Wird beim ersten Tauri-Start automatisch in ~/.zshrc eingerichtet:</span>
              <code>${appState.sourceLine}</code>
            </aside>`
      }

      ${notice ? `<p class="notice">${notice}</p>` : ""}
      ${error ? `<p class="error">${error}</p>` : ""}

      <section class="workspace">
        <form class="editor" id="alias-form">
          <div class="form-title">
            <h2>Alias erstellen</h2>
            <button class="primary-button" type="submit">Hinzufügen</button>
          </div>

          <label>
            Command Name
            <input name="name" value="${escapeHtml(form.name)}" placeholder="beerv2" autocomplete="off" />
          </label>

          <label>
            Ort / Datei / Befehl
            <span class="path-picker-row">
              <input name="path" value="${escapeHtml(form.path)}" placeholder="~/Desktop/projekte/beerv2_app" autocomplete="off" />
              <button class="picker-button" type="button" title="Datei auswählen" data-action="pick-path" data-target="create" data-kind="file">Datei</button>
              <button class="picker-button" type="button" title="Ordner auswählen" data-action="pick-path" data-target="create" data-kind="folder">Ordner</button>
            </span>
          </label>

          <label>
            Aktion
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
            <span>Vorschau</span>
            <code>${escapeHtml(formPreview())}</code>
          </div>
        </form>

        <section class="list" aria-label="Aliase">
          <div class="list-header">
            <h2>Deine Aliase</h2>
            <span>${aliases.length} Einträge</span>
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
                          <span class="created">Erstellt ${formatDate(alias.createdAt)}</span>
                        </div>
                        <button class="edit-button" title="Bearbeiten" data-action="edit" data-id="${alias.id}">Edit</button>
                        <button class="icon-button" title="Löschen" data-action="delete" data-id="${alias.id}">×</button>
                      </article>
                    `
                  )
                  .join("")
              : `<div class="empty-state">
                  <strong>Noch keine Aliase</strong>
                  <span>Leg links deinen ersten Command an.</span>
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

function renderEditModal() {
  if (!editForm || !editingId) return "";

  return `
    <section class="modal-layer" role="presentation">
      <form class="modal-card" id="edit-form" role="dialog" aria-modal="true" aria-labelledby="edit-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">Alias bearbeiten</p>
            <h2 id="edit-title">${escapeHtml(editForm.name || "Alias")}</h2>
          </div>
          <button class="ghost-button modal-close" type="button" data-action="close-edit">Schließen</button>
        </div>

        ${editError ? `<p class="modal-error">${escapeHtml(editError)}</p>` : ""}

        <label>
          Command Name
          <input name="edit-name" value="${escapeHtml(editForm.name)}" placeholder="beerv2" autocomplete="off" />
        </label>

        <label>
          Ort / Datei / Befehl
          <span class="path-picker-row">
            <input name="edit-path" value="${escapeHtml(editForm.path)}" placeholder="~/Desktop/projekte/beerv2_app" autocomplete="off" />
            <button class="picker-button" type="button" title="Datei auswählen" data-action="pick-path" data-target="edit" data-kind="file">Datei</button>
            <button class="picker-button" type="button" title="Ordner auswählen" data-action="pick-path" data-target="edit" data-kind="folder">Ordner</button>
          </span>
        </label>

        <label>
          Aktion
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
          <span>Vorschau</span>
          <code>${escapeHtml(editPreview())}</code>
        </div>

        <div class="modal-actions">
          <button class="ghost-button" type="button" data-action="close-edit">Abbrechen</button>
          <button class="primary-button" type="submit">Speichern</button>
        </div>
      </form>
    </section>
  `;
}

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

function escapeHtml(value: string) {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

void loadState();
