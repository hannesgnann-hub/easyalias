import "./styles.css";
import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  ChevronLeft,
  ChevronRight,
  CircleStop,
  Clock,
  Clock3,
  createIcons,
  FileDown,
  FileUp,
  Filter,
  FolderOpen,
  Pencil,
  Play,
  Plus,
  RotateCcw,
  Save,
  Search,
  SquareTerminal,
  Star,
  Tag,
  Tags,
  Terminal,
  Trash2,
  X
} from "lucide";

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
  favorite: boolean;
  createdAt: string;
  updatedAt: string;
};

type CommandFileCandidate = {
  id: string;
  name: string;
  command: string;
  sourceFile: string;
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
  importCandidates: CommandFileCandidate[];
};

type ImportResult = {
  state: AppState;
  importedCount: number;
  backupDir: string;
  warning?: string;
};

type BackupExportResult = {
  file: string;
  exportedCount: number;
};

type BackupImportResult = {
  state: AppState;
  importedCount: number;
  replacedCount: number;
};

type TrashEntry = {
  alias: AliasEntry;
  deletedAt: number;
};

type TrashMutationResult = {
  state: AppState;
  trash: TrashEntry[];
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

type PickerTarget = "create" | "edit" | "automation";
type PickerKind = "file" | "folder";
type BackupDialogMode = "export" | "import";
type AppView = "aliases" | "automations" | "timed-automations";
type AutomationStepKind = "command" | "wait";
type AutomationCommandBehavior = "wait" | "background";
type AutomationRunStepStatus = "pending" | "running" | "success" | "error" | "skipped";

type AutomationStep = {
  id: string;
  kind: AutomationStepKind;
  command: string;
  seconds: number;
  behavior: AutomationCommandBehavior;
};

type Automation = {
  id: string;
  name: string;
  path: string;
  steps: AutomationStep[];
  favorite: boolean;
  // Free-text label used to organize automations. Empty means ungrouped.
  group: string;
  createdAt: string;
  updatedAt: string;
};

type AutomationTrashEntry = {
  automation: Automation;
  deletedAt: number;
};

type AutomationTrashMutationResult = {
  automations: Automation[];
  trash: AutomationTrashEntry[];
};

type AutomationBackupImportResult = {
  automations: Automation[];
  importedCount: number;
  replacedCount: number;
};

type AutomationCommandResult = {
  exitCode: number | null;
  stdout: string;
  stderr: string;
  processId: number | null;
};

type AutomationRunStep = {
  status: AutomationRunStepStatus;
  output: string;
};

type AutomationRunState = {
  automationId: string;
  sessionId: string;
  running: boolean;
  cancelRequested: boolean;
  currentStep: number;
  error: string;
  steps: AutomationRunStep[];
};

// Filters are inferred from step commands (same patterns as the alias
// filter, minus favorites, which this automation model does not have yet)
// plus background steps and groups. "groups" switches the results area to
// a browsable group overview instead of filtering the automation list
// directly; a `group:<name>` value (picked from a group card or the
// dropdown) filters to that one group, where `group:` with an empty name
// means "ungrouped".
type AutomationStaticFilter = "all" | "favorites" | "background" | "git" | "docker" | "build" | "groups";
type AutomationFilter = AutomationStaticFilter | `group:${string}`;

const actionLabels: Record<AliasAction, string> = {
  navigate: "Go to Folder",
  open: "Open",
  execute: "Run",
  compile_gradle: "Gradle Build",
  compile_maven: "Maven Build",
  custom: "Custom Command"
};

const automationFilterLabels: Record<AutomationStaticFilter, string> = {
  all: "All automations",
  favorites: "Favorites",
  background: "Background",
  git: "Git",
  docker: "Docker",
  build: "Build",
  groups: "Group view"
};

function automationFilterLabel(filter: AutomationFilter): string {
  if (filter.startsWith("group:")) {
    const name = filter.slice("group:".length);
    return name ? `Group: ${name}` : "Ungrouped";
  }
  return automationFilterLabels[filter as AutomationStaticFilter];
}

function automationGroups(automations: Automation[]) {
  const names = new Set<string>();
  for (const automation of automations) {
    const trimmed = automation.group.trim();
    if (trimmed) names.add(trimmed);
  }
  return [...names].sort((left, right) => left.localeCompare(right));
}

const emptyForm: AliasForm = {
  name: "",
  path: "",
  action: "navigate",
  customCommand: ""
};

// Windows/cmd.exe starter shortcuts. Commands that wrap another batch file use
// `call`; generic wrappers forward user arguments with %*. Suggestions are
// ordered in themed groups so every nine-item page remains easy to scan.
const aliasSuggestions: AliasSuggestion[] = [
  {
    id: "git-status",
    name: "gs",
    path: "",
    action: "custom",
    customCommand: "git status --short --branch",
    description: "Compact Git status"
  },
  {
    id: "git-add-all",
    name: "gaa",
    path: "",
    action: "custom",
    customCommand: "git add --all",
    description: "Stage all Git changes"
  },
  {
    id: "git-commit",
    name: "gc",
    path: "",
    action: "custom",
    customCommand: "git commit %*",
    description: "Create a Git commit"
  },
  {
    id: "git-commit-message",
    name: "gcm",
    path: "",
    action: "custom",
    customCommand: "git commit -m \"%*\"",
    description: "Commit with a message"
  },
  {
    id: "git-push",
    name: "gp",
    path: "",
    action: "custom",
    customCommand: "git push %*",
    description: "Push the current branch"
  },
  {
    id: "git-pull-rebase",
    name: "gpl",
    path: "",
    action: "custom",
    customCommand: "git pull --rebase",
    description: "Pull with rebase"
  },
  {
    id: "git-branch",
    name: "gb",
    path: "",
    action: "custom",
    customCommand: "git branch",
    description: "List local Git branches"
  },
  {
    id: "git-switch",
    name: "gsw",
    path: "",
    action: "custom",
    customCommand: "git switch %*",
    description: "Switch Git branches"
  },
  {
    id: "git-diff",
    name: "gd",
    path: "",
    action: "custom",
    customCommand: "git diff",
    description: "Show unstaged changes"
  },
  {
    id: "git-diff-staged",
    name: "gds",
    path: "",
    action: "custom",
    customCommand: "git diff --staged",
    description: "Show staged changes"
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
    id: "git-stash",
    name: "gstash",
    path: "",
    action: "custom",
    customCommand: "git stash push %*",
    description: "Stash current changes"
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
    id: "docker-compose-down",
    name: "dcd",
    path: "",
    action: "custom",
    customCommand: "docker compose down",
    description: "Stop Docker Compose"
  },
  {
    id: "docker-compose-logs",
    name: "dcl",
    path: "",
    action: "custom",
    customCommand: "docker compose logs -f %*",
    description: "Follow Compose logs"
  },
  {
    id: "docker-compose-build",
    name: "dcb",
    path: "",
    action: "custom",
    customCommand: "docker compose build %*",
    description: "Build Compose services"
  },
  {
    id: "docker-compose-restart",
    name: "dcr",
    path: "",
    action: "custom",
    customCommand: "docker compose restart %*",
    description: "Restart Compose services"
  },
  {
    id: "docker-ps",
    name: "dps",
    path: "",
    action: "custom",
    customCommand: "docker ps",
    description: "List running containers"
  },
  {
    id: "docker-images",
    name: "di",
    path: "",
    action: "custom",
    customCommand: "docker images",
    description: "List local Docker images"
  },
  {
    id: "docker-disk-usage",
    name: "ddf",
    path: "",
    action: "custom",
    customCommand: "docker system df",
    description: "Show Docker disk usage"
  },
  {
    id: "docker-exec",
    name: "dex",
    path: "",
    action: "custom",
    customCommand: "docker exec -it %*",
    description: "Run a command in a container"
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
    id: "maven-wrapper-build",
    name: "mvnb",
    path: "",
    action: "custom",
    customCommand: "call mvnw.cmd clean package",
    description: "Build with Maven wrapper"
  },
  {
    id: "maven-wrapper-test",
    name: "mvnt",
    path: "",
    action: "custom",
    customCommand: "call mvnw.cmd test",
    description: "Run Maven tests"
  },
  {
    id: "npm-install",
    name: "ni",
    path: "",
    action: "custom",
    customCommand: "call npm install",
    description: "Install npm dependencies"
  },
  {
    id: "npm-run-dev",
    name: "nrd",
    path: "",
    action: "custom",
    customCommand: "call npm run dev",
    description: "Start the npm dev script"
  },
  {
    id: "npm-run-build",
    name: "nrb",
    path: "",
    action: "custom",
    customCommand: "call npm run build",
    description: "Run the npm build script"
  },
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
    id: "python-server",
    name: "serve",
    path: "",
    action: "custom",
    customCommand: "python -m http.server",
    description: "Serve the current folder"
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
  }
];

// Global UI state. For this prototype we keep state in module-level variables
// and re-render the app when larger UI structure changes.
let appState: AppState = {
  aliases: [],
  configFile: "~/.easyalias/config.json",
  commandDir: "~/.easyalias/bin",
  pathEntry: "~/.easyalias/bin",
  pathConfigured: false,
  importCandidates: []
};

let form: AliasForm = { ...emptyForm };
let editForm: AliasForm | null = null;
let editingId: string | null = null;
// Suggestions remain out of the main workflow until the user expands them.
let suggestionsExpanded = false;
// Nine cards fill the three-column layout and keep every page the same height.
const suggestionPageSize = 9;
let suggestionPage = 1;
let selectedImportIds = new Set<string>();
let importBusy = false;
// Manual imports share the first-start modal but close without writing the
// one-time import marker.
let manualImportOpen = false;
let notice = "";
let error = "";
let messageDismissTimer: ReturnType<typeof setTimeout> | null = null;
let scheduledMessageKey = "";
let editError = "";
let importError = "";
// Backup import/export has its own modal state so it never interferes with the
// first-start legacy command-file migration flow above.
let backupDialogMode: BackupDialogMode | null = null;
let backupCandidates: AliasEntry[] = [];
let selectedBackupIds = new Set<string>();
let backupFilePath = "";
let backupBusy = false;
let backupError = "";
// Deleted aliases remain recoverable for 30 days. Native builds persist these
// entries in ~/.easyalias/trash.json; browser preview mirrors them locally.
let trashEntries: TrashEntry[] = [];
let trashOpen = false;
let trashBusy = false;
let trashError = "";
// Automations have their own view and storage. The editor keeps a detached
// draft so cancelling never mutates a saved workflow.
let currentView: AppView = "aliases";
let automations: Automation[] = [];
let automationEditor: Automation | null = null;
let automationBusy = false;
let automationError = "";
let automationRun: AutomationRunState | null = null;
let automationSearchQuery = "";
let automationFilter: AutomationFilter = "all";
// Only one card's quick group-picker is open at a time; the editor's picker
// is separate since it lives in its own modal.
let automationGroupPickerId: string | null = null;
let automationEditorGroupPickerOpen = false;
// Automation backups deliberately use their own state and file format. This
// prevents a workflow backup from being mistaken for an alias backup while
// retaining the same selective export/import experience.
let automationBackupDialogMode: BackupDialogMode | null = null;
let automationBackupCandidates: Automation[] = [];
let selectedAutomationBackupIds = new Set<string>();
let automationBackupFilePath = "";
let automationBackupBusy = false;
let automationBackupError = "";
// Deleted workflows use the same 30-day recovery model as aliases, but live
// in their own file so alias and automation data can never be mixed.
let automationTrashEntries: AutomationTrashEntry[] = [];
let automationTrashOpen = false;
let automationTrashBusy = false;
let automationTrashError = "";

const trashRetentionSeconds = 30 * 24 * 60 * 60;

// Vite mounts the app into <main id="app"> from index.html.
const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
  throw new Error("App container not found");
}

const appElement = app;
const repoUrl = "https://github.com/hannesgnann-hub/easyalias";
const redditUrl = "https://www.reddit.com/r/easyalias/";
const websiteUrl = "https://easyalias.org";
const sponsorUrl = "https://github.com/sponsors/hannesgnann-hub";

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

    if (target === "edit") {
      updateEditForm("path", selected);
      const input = document.querySelector<HTMLInputElement>('input[name="edit-path"]');
      if (input) input.value = selected;
      return;
    }

    if (automationEditor) {
      automationEditor = { ...automationEditor, path: selected };
      const input = document.querySelector<HTMLInputElement>('input[name="automation-path"]');
      if (input) input.value = selected;
    }
  } catch (pickerError) {
    const message = `Picker could not be opened: ${String(pickerError)}`;
    if (target === "automation") {
      automationError = message;
    } else if (target === "edit") {
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
      try {
        trashEntries = await invokeCommand<TrashEntry[]>("list_trash");
      } catch (trashLoadError) {
        // Alias loading remains usable even if the separate trash file needs
        // attention; surface the problem without replacing native state.
        trashEntries = [];
        error = `Trash could not be loaded: ${String(trashLoadError)}`;
      }
      try {
        automations = await invokeCommand<Automation[]>("load_automations");
      } catch (automationLoadError) {
        automations = [];
        error = `Automations could not be loaded: ${String(automationLoadError)}`;
      }
      selectedImportIds = new Set(appState.importCandidates.map((candidate) => candidate.id));
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
    appState = {
      ...appState,
      ...(JSON.parse(saved) as Partial<AppState>),
      importCandidates: []
    };
  }
  const savedTrash = localStorage.getItem("easyalias-trash");
  if (savedTrash) {
    const cutoff = Math.floor(Date.now() / 1000) - trashRetentionSeconds;
    trashEntries = (JSON.parse(savedTrash) as TrashEntry[])
      .filter((entry) => entry.deletedAt > cutoff)
      .sort((left, right) => right.deletedAt - left.deletedAt);
    localStorage.setItem("easyalias-trash", JSON.stringify(trashEntries));
  }
  const savedAutomations = localStorage.getItem("easyalias-automations");
  if (savedAutomations) {
    automations = (JSON.parse(savedAutomations) as Automation[]).map((automation) => ({
      ...automation,
      group: automation.group ?? ""
    }));
  }

  render();
}

function saveBrowserTrash() {
  localStorage.setItem("easyalias-trash", JSON.stringify(trashEntries));
}

function saveBrowserAutomationTrash() {
  localStorage.setItem("easyalias-automation-trash", JSON.stringify(automationTrashEntries));
}

// Persists current aliases. Tauri writes real files; browser preview only writes localStorage.
async function saveState() {
  clearMessages();

  const aliases = [...appState.aliases].sort(compareAliases);

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
function cancelMessageDismissal() {
  if (messageDismissTimer !== null) {
    clearTimeout(messageDismissTimer);
    messageDismissTimer = null;
  }

  scheduledMessageKey = "";
}

function clearMessages() {
  cancelMessageDismissal();
  notice = "";
  error = "";
}

function clearRenderedMessages() {
  document.querySelector(".notice")?.remove();
  document.querySelector(".error")?.remove();
}

function dismissMessage() {
  clearMessages();
  render();
}

// Every new global status message gets a fresh three-second lifetime. Re-renders
// with the same message keep the existing deadline instead of extending it.
function scheduleMessageDismissal() {
  const messageKey = error ? `error:${error}` : notice ? `notice:${notice}` : "";

  if (!messageKey) {
    cancelMessageDismissal();
    return;
  }

  if (messageDismissTimer !== null && scheduledMessageKey === messageKey) return;

  cancelMessageDismissal();
  scheduledMessageKey = messageKey;
  messageDismissTimer = setTimeout(() => {
    messageDismissTimer = null;
    scheduledMessageKey = "";
    notice = "";
    error = "";
    render();
  }, 3000);
}

function toggleSuggestions() {
  suggestionsExpanded = !suggestionsExpanded;
  render();
}

// Page numbers come from HTML data attributes, so normalize them before the
// next render applies the upper bound for the currently available suggestions.
function showSuggestionPage(page: number) {
  if (!Number.isFinite(page)) return;
  suggestionPage = Math.max(1, Math.floor(page));
  render();
}

// Rescan legacy .cmd/.bat aliases even after the first-start prompt was handled.
// Rust filters managed names and remains authoritative for PATH and file access.
async function openCommandFileImport() {
  if (importBusy) return;
  clearMessages();
  importError = "";
  importBusy = true;
  render();

  try {
    appState = await invokeCommand<AppState>("scan_command_file_import");
    selectedImportIds = new Set(appState.importCandidates.map((candidate) => candidate.id));
    manualImportOpen = appState.importCandidates.length > 0;

    if (!manualImportOpen) {
      notice = "No new command files found in your PATH folders.";
    }
  } catch (scanError) {
    error = String(scanError);
  }

  importBusy = false;
  render();
}

function closeManualImport() {
  if (importBusy) return;
  appState = { ...appState, importCandidates: [] };
  selectedImportIds.clear();
  importError = "";
  manualImportOpen = false;
  render();
}

async function dismissCommandFileImport() {
  if (importBusy) return;
  importBusy = true;
  importError = "";
  render();

  try {
    appState = await invokeCommand<AppState>("dismiss_command_file_import");
    selectedImportIds.clear();
    manualImportOpen = false;
    notice = "Existing command files were left unchanged.";
  } catch (dismissError) {
    importError = String(dismissError);
  }

  importBusy = false;
  render();
}

async function importSelectedCommandFiles(event: SubmitEvent) {
  event.preventDefault();
  if (importBusy) return;
  importError = "";

  if (selectedImportIds.size === 0) {
    importError = "Select at least one command file to import.";
    render();
    return;
  }

  importBusy = true;
  render();

  try {
    const result = await invokeCommand<ImportResult>("import_command_files", {
      selectedIds: [...selectedImportIds],
      timestamp: nowIso()
    });
    appState = result.state;
    selectedImportIds.clear();
    manualImportOpen = false;
    notice = `${result.importedCount} command files imported. Backup: ${result.backupDir}`;
    if (result.warning) error = result.warning;
  } catch (importFailure) {
    importError = String(importFailure);
  }

  importBusy = false;
  render();
}

// Turn one suggestion into a real AliasEntry immediately. The duplicate guard
// also protects against a second click that arrives while a save is in flight.
function openBackupExport() {
  clearMessages();
  backupError = "";
  backupFilePath = "";
  backupCandidates = [...appState.aliases].sort(compareAliases);
  selectedBackupIds = new Set(backupCandidates.map((alias) => alias.id));
  backupDialogMode = "export";
  render();
}

function openBackupImport() {
  clearMessages();
  backupError = "";
  backupFilePath = "";
  backupCandidates = [];
  selectedBackupIds.clear();
  backupDialogMode = "import";
  render();
}

function closeBackupDialog() {
  if (backupBusy) return;
  backupDialogMode = null;
  backupCandidates = [];
  selectedBackupIds.clear();
  backupFilePath = "";
  backupError = "";
  render();
}

async function chooseBackupFile() {
  if (backupBusy || backupDialogMode !== "import") return;
  backupError = "";

  if (!isTauriRuntime()) {
    backupError = "Backup files can only be opened in the Tauri app.";
    render();
    return;
  }

  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "EasyAlias Backup", extensions: ["json"] }]
    });
    if (typeof selected === "string") await inspectBackupFile(selected);
  } catch (fileError) {
    backupError = `Backup could not be selected: ${String(fileError)}`;
    render();
  }
}

async function inspectBackupFile(path: string) {
  if (backupBusy || backupDialogMode !== "import") return;
  backupBusy = true;
  backupError = "";
  render();

  try {
    backupCandidates = await invokeCommand<AliasEntry[]>("inspect_alias_backup", { path });
    backupFilePath = path;
    selectedBackupIds = new Set(backupCandidates.map((alias) => alias.id));
  } catch (inspectError) {
    backupCandidates = [];
    selectedBackupIds.clear();
    backupFilePath = "";
    backupError = String(inspectError);
  }

  backupBusy = false;
  render();
}

async function exportSelectedAliases(event: SubmitEvent) {
  event.preventDefault();
  if (backupBusy || backupDialogMode !== "export") return;
  backupError = "";

  if (selectedBackupIds.size === 0) {
    backupError = "Select at least one alias to export.";
    render();
    return;
  }
  if (!isTauriRuntime()) {
    backupError = "Backups can only be exported in the Tauri app.";
    render();
    return;
  }

  try {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const destination = await save({
      defaultPath: `EasyAlias-backup-${new Date().toISOString().slice(0, 10)}.json`,
      filters: [{ name: "EasyAlias Backup", extensions: ["json"] }]
    });
    if (typeof destination !== "string") return;

    backupBusy = true;
    render();
    const result = await invokeCommand<BackupExportResult>("export_alias_backup", {
      selectedIds: [...selectedBackupIds],
      destination,
      exportedAt: nowIso()
    });
    closeBackupDialogAfterSuccess();
    notice = `${result.exportedCount} aliases exported to ${result.file}.`;
  } catch (exportError) {
    backupError = String(exportError);
  }

  backupBusy = false;
  render();
}

async function importSelectedBackupAliases(event: SubmitEvent) {
  event.preventDefault();
  if (backupBusy || backupDialogMode !== "import") return;
  backupError = "";

  if (!backupFilePath || selectedBackupIds.size === 0) {
    backupError = backupFilePath
      ? "Select at least one alias to import."
      : "Choose or drop an EasyAlias backup first.";
    render();
    return;
  }

  backupBusy = true;
  render();
  try {
    const result = await invokeCommand<BackupImportResult>("import_alias_backup", {
      path: backupFilePath,
      selectedIds: [...selectedBackupIds],
      importedAt: nowIso()
    });
    appState = result.state;
    closeBackupDialogAfterSuccess();
    const replacementNote = result.replacedCount
      ? ` ${result.replacedCount} existing aliases replaced.`
      : "";
    notice = `${result.importedCount} aliases imported.${replacementNote}`;
  } catch (backupImportError) {
    backupError = String(backupImportError);
  }

  backupBusy = false;
  render();
}

function closeBackupDialogAfterSuccess() {
  backupDialogMode = null;
  backupCandidates = [];
  selectedBackupIds.clear();
  backupFilePath = "";
  backupError = "";
}

// Save a suggestion immediately. Suggestions with an existing alias name are
// hidden in the UI, while the duplicate check also protects against stale clicks.
async function useSuggestion(id: string) {
  const suggestion = aliasSuggestions.find((item) => item.id === id);
  if (!suggestion) return;

  if (appState.aliases.some((alias) => alias.name.toLowerCase() === suggestion.name.toLowerCase())) {
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
    favorite: false,
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
    favorite: false,
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
    favorite: Boolean(existing.favorite),
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

// Deleting now moves the alias into the recoverable 30-day trash instead of
// removing it immediately from disk.
async function deleteAlias(id: string) {
  const existing = appState.aliases.find((alias) => alias.id === id);
  if (!existing) return;

  clearMessages();

  if (isTauriRuntime()) {
    try {
      const result = await invokeCommand<TrashMutationResult>("move_alias_to_trash", { id });
      appState = result.state;
      trashEntries = result.trash;
    } catch (deleteError) {
      error = String(deleteError);
      render();
      return;
    }
  } else {
    appState = {
      ...appState,
      aliases: appState.aliases.filter((alias) => alias.id !== id)
    };
    trashEntries = [
      { alias: existing, deletedAt: Math.floor(Date.now() / 1000) },
      ...trashEntries.filter((entry) => entry.alias.id !== id)
    ];
    localStorage.setItem("easyalias-state", JSON.stringify(appState));
    saveBrowserTrash();
  }

  if (editingId === id) {
    editingId = null;
    editForm = null;
    editError = "";
  }

  notice = `Alias "${existing.name}" moved to Trash.`;
  render();
}

async function openTrash() {
  if (trashBusy) return;
  clearMessages();
  trashError = "";

  if (isTauriRuntime()) {
    try {
      trashEntries = await invokeCommand<TrashEntry[]>("list_trash");
    } catch (loadError) {
      error = `Trash could not be opened: ${String(loadError)}`;
      render();
      return;
    }
  }

  trashOpen = true;
  render();
}

function closeTrash() {
  if (trashBusy) return;
  trashOpen = false;
  trashError = "";
  render();
}

async function restoreTrashAlias(id: string) {
  if (trashBusy) return;
  const entry = trashEntries.find((item) => item.alias.id === id);
  if (!entry) return;
  trashBusy = true;
  trashError = "";
  render();

  try {
    if (isTauriRuntime()) {
      const result = await invokeCommand<TrashMutationResult>("restore_trash_alias", { id });
      appState = result.state;
      trashEntries = result.trash;
    } else {
      if (appState.aliases.some((alias) => alias.name.toLowerCase() === entry.alias.name.toLowerCase())) {
        throw new Error(`Alias "${entry.alias.name}" already exists.`);
      }
      appState = {
        ...appState,
        aliases: [...appState.aliases, entry.alias].sort(compareAliases)
      };
      trashEntries = trashEntries.filter((item) => item.alias.id !== id);
      localStorage.setItem("easyalias-state", JSON.stringify(appState));
      saveBrowserTrash();
    }
    notice = `Alias "${entry.alias.name}" restored.`;
  } catch (restoreError) {
    trashError = String(restoreError);
  } finally {
    trashBusy = false;
    render();
  }
}

async function permanentlyDeleteTrashAlias(id: string) {
  if (trashBusy) return;
  const entry = trashEntries.find((item) => item.alias.id === id);
  if (!entry) return;
  if (!window.confirm(`Permanently delete alias "${entry.alias.name}"? This cannot be undone.`)) return;

  trashBusy = true;
  trashError = "";
  render();
  try {
    if (isTauriRuntime()) {
      trashEntries = await invokeCommand<TrashEntry[]>("permanently_delete_trash_alias", { id });
    } else {
      trashEntries = trashEntries.filter((item) => item.alias.id !== id);
      saveBrowserTrash();
    }
    notice = `Alias "${entry.alias.name}" permanently deleted.`;
  } catch (deleteError) {
    trashError = String(deleteError);
  } finally {
    trashBusy = false;
    render();
  }
}

async function emptyTrash() {
  if (trashBusy || !trashEntries.length) return;
  if (!window.confirm(`Permanently delete all ${trashEntries.length} aliases in Trash? This cannot be undone.`)) return;

  trashBusy = true;
  trashError = "";
  render();
  try {
    if (isTauriRuntime()) {
      trashEntries = await invokeCommand<TrashEntry[]>("empty_trash");
    } else {
      trashEntries = [];
      saveBrowserTrash();
    }
    notice = "Trash emptied.";
  } catch (emptyError) {
    trashError = String(emptyError);
  } finally {
    trashBusy = false;
    render();
  }
}

async function toggleFavorite(id: string) {
  const existing = appState.aliases.find((alias) => alias.id === id);
  if (!existing) return;

  appState = {
    ...appState,
    aliases: appState.aliases.map((alias) =>
      alias.id === id
        ? { ...alias, favorite: !Boolean(alias.favorite), updatedAt: nowIso() }
        : alias
    )
  };
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

function formatDeletedDate(value: number) {
  return new Intl.DateTimeFormat("en-US", {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(new Date(value * 1000));
}

function trashDaysRemaining(value: number) {
  const purgeAt = value + trashRetentionSeconds;
  return Math.max(1, Math.ceil((purgeAt - Date.now() / 1000) / (24 * 60 * 60)));
}

// Favorites are grouped first; each group remains predictable and alphabetical.
function compareAliases(left: AliasEntry, right: AliasEntry) {
  const favoriteDifference = Number(Boolean(right.favorite)) - Number(Boolean(left.favorite));
  return favoriteDifference || left.name.localeCompare(right.name);
}

function compareAutomations(left: Automation, right: Automation) {
  const favoriteDifference = Number(Boolean(right.favorite)) - Number(Boolean(left.favorite));
  return favoriteDifference || left.name.localeCompare(right.name);
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

function createAutomationStep(kind: AutomationStepKind): AutomationStep {
  return {
    id: createId(),
    kind,
    command: "",
    seconds: kind === "wait" ? 10 : 0,
    behavior: "wait"
  };
}

function openAutomationsView() {
  clearMessages();
  automationError = "";
  automationGroupPickerId = null;
  currentView = "automations";
  render();
}

function closeAutomationsView() {
  if (automationRun?.running) return;
  automationEditor = null;
  automationError = "";
  automationRun = null;
  automationGroupPickerId = null;
  currentView = "aliases";
  render();
}

// Placeholder view for the timed-automations feature being built on this
// branch. Intentionally empty; fill in as the feature takes shape.
function openTimedAutomationsView() {
  clearMessages();
  currentView = "timed-automations";
  render();
}

function closeTimedAutomationsView() {
  currentView = "aliases";
  render();
}

function renderTimedAutomationsView() {
  appElement.innerHTML = `
    <section class="shell automation-shell">
      <header class="topbar automation-topbar">
        <div>
          <p class="eyebrow">Coming Soon</p>
          <h1>Timed Automations</h1>
        </div>
        <div class="topbar-actions">
          <button class="header-icon-button" type="button" title="Back to aliases" aria-label="Back to aliases" data-action="close-timed-automations"><i data-lucide="arrow-left"></i></button>
        </div>
      </header>

      <div class="empty-state">
        <strong>Nothing here yet</strong>
        <span>This page is a placeholder for the timed-automations feature.</span>
      </div>

      <aside class="support-banner" aria-label="Support EasyAlias"><span>Support EasyAlias development</span><a href="${sponsorUrl}" target="_blank" rel="noreferrer" data-external-link>Become a sponsor</a></aside>
      <footer class="app-footer"><a href="${repoUrl}" target="_blank" rel="noreferrer" data-external-link>© Hannes Gnann</a><span aria-hidden="true">-</span><a href="${redditUrl}" target="_blank" rel="noreferrer" data-external-link>Reddit</a><span aria-hidden="true">-</span><a href="${websiteUrl}" target="_blank" rel="noreferrer" data-external-link>Website</a></footer>
    </section>`;

  createIcons({
    icons: { ArrowLeft },
    attrs: { "aria-hidden": "true", width: "20", height: "20", "stroke-width": "2" }
  });

  document.querySelector<HTMLButtonElement>('[data-action="close-timed-automations"]')?.addEventListener("click", closeTimedAutomationsView);
  document.querySelectorAll<HTMLAnchorElement>("[data-external-link]").forEach((link) => link.addEventListener("click", openExternalLink));
}

function openAutomationEditor(id?: string) {
  const existing = id ? automations.find((automation) => automation.id === id) : null;
  const timestamp = nowIso();
  automationEditor = existing
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
  automationError = "";
  automationEditorGroupPickerOpen = false;
  render();
}

function closeAutomationEditor() {
  if (automationBusy) return;
  automationEditor = null;
  automationError = "";
  automationEditorGroupPickerOpen = false;
  render();
}

function updateAutomationEditor<K extends "name" | "path" | "group">(key: K, value: Automation[K]) {
  if (!automationEditor) return;
  automationEditor = { ...automationEditor, [key]: value };
  automationError = "";
  document.querySelector(".automation-editor .modal-error")?.remove();
}

function updateAutomationStep<K extends keyof AutomationStep>(
  index: number,
  key: K,
  value: AutomationStep[K],
  rerender = false
) {
  if (!automationEditor || !automationEditor.steps[index]) return;
  const steps = automationEditor.steps.map((step, stepIndex) =>
    stepIndex === index ? { ...step, [key]: value } : step
  );
  automationEditor = { ...automationEditor, steps };
  automationError = "";
  if (rerender) render();
}

function addAutomationStep(kind: AutomationStepKind) {
  if (!automationEditor) return;
  automationEditor = {
    ...automationEditor,
    steps: [...automationEditor.steps, createAutomationStep(kind)]
  };
  automationError = "";
  render();
}

function moveAutomationStep(index: number, offset: number) {
  if (!automationEditor) return;
  const target = index + offset;
  if (target < 0 || target >= automationEditor.steps.length) return;
  const steps = [...automationEditor.steps];
  [steps[index], steps[target]] = [steps[target], steps[index]];
  automationEditor = { ...automationEditor, steps };
  render();
}

function removeAutomationStep(index: number) {
  if (!automationEditor || automationEditor.steps.length === 1) {
    automationError = "An automation needs at least one step.";
    render();
    return;
  }
  automationEditor = {
    ...automationEditor,
    steps: automationEditor.steps.filter((_, stepIndex) => stepIndex !== index)
  };
  automationError = "";
  render();
}

function validateAutomation(automation: Automation) {
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

async function persistAutomations(next: Automation[]) {
  if (isTauriRuntime()) {
    automations = await invokeCommand<Automation[]>("save_automations", { automations: next });
    return;
  }
  automations = next;
  localStorage.setItem("easyalias-automations", JSON.stringify(next));
}

function toggleAutomationGroupPicker(id: string) {
  automationGroupPickerId = automationGroupPickerId === id ? null : id;
  refreshAutomationResults();
}

async function assignAutomationGroup(id: string, group: string) {
  const automation = automations.find((item) => item.id === id);
  if (!automation) return;

  const trimmedGroup = group.trim();
  if (trimmedGroup.length > 60) {
    error = "The group label must be at most 60 characters.";
    render();
    return;
  }

  automationGroupPickerId = null;
  try {
    await persistAutomations(
      automations.map((item) => (item.id === id ? { ...item, group: trimmedGroup, updatedAt: nowIso() } : item))
    );
    notice = trimmedGroup
      ? `Moved "${automation.name}" to "${trimmedGroup}".`
      : `Removed "${automation.name}" from its group.`;
  } catch (assignError) {
    error = String(assignError);
  }
  render();
}

async function saveAutomation(event: SubmitEvent) {
  event.preventDefault();
  if (!automationEditor || automationBusy) return;

  automationError = validateAutomation(automationEditor);
  if (automationError) {
    render();
    return;
  }
  const duplicate = automations.find(
    (automation) =>
      automation.id !== automationEditor?.id &&
      automation.name.trim().toLocaleLowerCase() === automationEditor?.name.trim().toLocaleLowerCase()
  );
  if (duplicate) {
    automationError = `Automation "${automationEditor.name.trim()}" already exists.`;
    render();
    return;
  }

  automationBusy = true;
  render();
  const savedAutomation: Automation = {
    ...automationEditor,
    name: automationEditor.name.trim(),
    path: automationEditor.path.trim(),
    group: automationEditor.group.trim(),
    steps: automationEditor.steps.map((step) => ({
      ...step,
      command: step.kind === "command" ? step.command.trim() : "",
      seconds: step.kind === "wait" ? Math.floor(step.seconds) : 0
    })),
    updatedAt: nowIso()
  };
  const exists = automations.some((automation) => automation.id === savedAutomation.id);
  const next = exists
    ? automations.map((automation) =>
        automation.id === savedAutomation.id ? savedAutomation : automation
      )
    : [...automations, savedAutomation];

  try {
    await persistAutomations(next);
    automationEditor = null;
    notice = `Automation "${savedAutomation.name}" saved.`;
  } catch (saveError) {
    automationError = String(saveError);
  } finally {
    automationBusy = false;
    render();
  }
}

async function deleteAutomation(id: string) {
  if (automationBusy || automationRun?.running) return;
  const automation = automations.find((item) => item.id === id);
  if (!automation || !window.confirm(`Move automation "${automation.name}" to Trash?`)) return;
  automationBusy = true;
  try {
    if (isTauriRuntime()) {
      const result = await invokeCommand<AutomationTrashMutationResult>("move_automation_to_trash", { id });
      automations = result.automations;
      automationTrashEntries = result.trash;
    } else {
      const nextAutomations = automations.filter((item) => item.id !== id);
      await persistAutomations(nextAutomations);
      automationTrashEntries = [
        { automation, deletedAt: Math.floor(Date.now() / 1000) },
        ...automationTrashEntries.filter((entry) => entry.automation.id !== id)
      ].sort((left, right) => right.deletedAt - left.deletedAt);
      saveBrowserAutomationTrash();
    }
    notice = `Automation "${automation.name}" moved to Trash.`;
  } catch (deleteError) {
    error = String(deleteError);
  } finally {
    automationBusy = false;
    render();
  }
}

async function openAutomationTrash() {
  clearMessages();
  automationTrashError = "";
  automationTrashOpen = true;

  if (isTauriRuntime()) {
    automationTrashBusy = true;
    render();
    try {
      automationTrashEntries = await invokeCommand<AutomationTrashEntry[]>("list_automation_trash");
    } catch (trashLoadError) {
      automationTrashError = String(trashLoadError);
    } finally {
      automationTrashBusy = false;
    }
  } else {
    const cutoff = Math.floor(Date.now() / 1000) - trashRetentionSeconds;
    automationTrashEntries = automationTrashEntries
      .filter((entry) => entry.deletedAt > cutoff)
      .sort((left, right) => right.deletedAt - left.deletedAt);
    saveBrowserAutomationTrash();
  }

  render();
}

function closeAutomationTrash() {
  if (automationTrashBusy) return;
  automationTrashOpen = false;
  automationTrashError = "";
  render();
}

async function restoreTrashAutomation(id: string) {
  if (automationTrashBusy) return;
  const entry = automationTrashEntries.find((item) => item.automation.id === id);
  if (!entry) return;

  automationTrashBusy = true;
  automationTrashError = "";
  render();
  try {
    if (isTauriRuntime()) {
      const result = await invokeCommand<AutomationTrashMutationResult>("restore_trash_automation", { id });
      automations = result.automations;
      automationTrashEntries = result.trash;
    } else {
      const duplicate = automations.find(
        (automation) =>
          automation.id === entry.automation.id ||
          automation.name.trim().toLocaleLowerCase() === entry.automation.name.trim().toLocaleLowerCase()
      );
      if (duplicate) {
        throw new Error(`Automation "${entry.automation.name}" already exists.`);
      }
      await persistAutomations([...automations, entry.automation].sort(compareAutomations));
      automationTrashEntries = automationTrashEntries.filter((item) => item.automation.id !== id);
      saveBrowserAutomationTrash();
    }
    notice = `Automation "${entry.automation.name}" restored.`;
  } catch (restoreError) {
    automationTrashError = String(restoreError);
  } finally {
    automationTrashBusy = false;
    render();
  }
}

async function permanentlyDeleteTrashAutomation(id: string) {
  if (automationTrashBusy) return;
  const entry = automationTrashEntries.find((item) => item.automation.id === id);
  if (!entry || !window.confirm(`Permanently delete automation "${entry.automation.name}"? This cannot be undone.`)) {
    return;
  }

  automationTrashBusy = true;
  automationTrashError = "";
  render();
  try {
    if (isTauriRuntime()) {
      automationTrashEntries = await invokeCommand<AutomationTrashEntry[]>(
        "permanently_delete_trash_automation",
        { id }
      );
    } else {
      automationTrashEntries = automationTrashEntries.filter((item) => item.automation.id !== id);
      saveBrowserAutomationTrash();
    }
  } catch (deleteError) {
    automationTrashError = String(deleteError);
  } finally {
    automationTrashBusy = false;
    render();
  }
}

async function emptyAutomationTrash() {
  if (automationTrashBusy || automationTrashEntries.length === 0) return;
  const count = automationTrashEntries.length;
  if (!window.confirm(`Permanently delete all ${count} automation${count === 1 ? "" : "s"} in Trash? This cannot be undone.`)) {
    return;
  }

  automationTrashBusy = true;
  automationTrashError = "";
  render();
  try {
    if (isTauriRuntime()) {
      automationTrashEntries = await invokeCommand<AutomationTrashEntry[]>("empty_automation_trash");
    } else {
      automationTrashEntries = [];
      saveBrowserAutomationTrash();
    }
  } catch (emptyError) {
    automationTrashError = String(emptyError);
  } finally {
    automationTrashBusy = false;
    render();
  }
}

async function toggleAutomationFavorite(id: string) {
  if (automationBusy || automationRun?.running) return;
  const automation = automations.find((item) => item.id === id);
  if (!automation) return;

  automationBusy = true;
  try {
    await persistAutomations(
      automations.map((item) =>
        item.id === id
          ? { ...item, favorite: !Boolean(item.favorite), updatedAt: nowIso() }
          : item
      )
    );
  } catch (favoriteError) {
    error = String(favoriteError);
  } finally {
    automationBusy = false;
    render();
  }
}

function openAutomationBackupExport() {
  clearMessages();
  automationBackupError = "";
  automationBackupFilePath = "";
  automationBackupCandidates = [...automations].sort(compareAutomations);
  selectedAutomationBackupIds = new Set(automationBackupCandidates.map((automation) => automation.id));
  automationBackupDialogMode = "export";
  renderAutomationsView();
}

function openAutomationBackupImport() {
  clearMessages();
  automationBackupError = "";
  automationBackupFilePath = "";
  automationBackupCandidates = [];
  selectedAutomationBackupIds.clear();
  automationBackupDialogMode = "import";
  renderAutomationsView();
}

function closeAutomationBackupDialog() {
  if (automationBackupBusy) return;
  automationBackupDialogMode = null;
  automationBackupCandidates = [];
  selectedAutomationBackupIds.clear();
  automationBackupFilePath = "";
  automationBackupError = "";
  renderAutomationsView();
}

function closeAutomationBackupDialogAfterSuccess() {
  automationBackupDialogMode = null;
  automationBackupCandidates = [];
  selectedAutomationBackupIds.clear();
  automationBackupFilePath = "";
  automationBackupError = "";
}

async function chooseAutomationBackupFile() {
  if (automationBackupBusy || automationBackupDialogMode !== "import") return;
  automationBackupError = "";

  if (!isTauriRuntime()) {
    automationBackupError = "Automation backup files can only be opened in the Tauri app.";
    renderAutomationsView();
    return;
  }

  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "EasyAlias Automation Backup", extensions: ["json"] }]
    });
    if (typeof selected === "string") await inspectAutomationBackupFile(selected);
  } catch (fileError) {
    automationBackupError = `Automation backup could not be selected: ${String(fileError)}`;
    renderAutomationsView();
  }
}

async function inspectAutomationBackupFile(path: string) {
  if (automationBackupBusy || automationBackupDialogMode !== "import") return;
  automationBackupBusy = true;
  automationBackupError = "";
  renderAutomationsView();

  try {
    automationBackupCandidates = await invokeCommand<Automation[]>("inspect_automation_backup", { path });
    automationBackupFilePath = path;
    selectedAutomationBackupIds = new Set(automationBackupCandidates.map((automation) => automation.id));
  } catch (inspectError) {
    automationBackupCandidates = [];
    selectedAutomationBackupIds.clear();
    automationBackupFilePath = "";
    automationBackupError = String(inspectError);
  }

  automationBackupBusy = false;
  renderAutomationsView();
}

async function exportSelectedAutomations(event: SubmitEvent) {
  event.preventDefault();
  if (automationBackupBusy || automationBackupDialogMode !== "export") return;
  automationBackupError = "";

  if (selectedAutomationBackupIds.size === 0) {
    automationBackupError = "Select at least one automation to export.";
    renderAutomationsView();
    return;
  }
  if (!isTauriRuntime()) {
    automationBackupError = "Automation backups can only be exported in the Tauri app.";
    renderAutomationsView();
    return;
  }

  try {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const destination = await save({
      defaultPath: `EasyAlias-automations-backup-${new Date().toISOString().slice(0, 10)}.json`,
      filters: [{ name: "EasyAlias Automation Backup", extensions: ["json"] }]
    });
    if (typeof destination !== "string") return;

    automationBackupBusy = true;
    renderAutomationsView();
    const result = await invokeCommand<BackupExportResult>("export_automation_backup", {
      selectedIds: [...selectedAutomationBackupIds],
      destination,
      exportedAt: nowIso()
    });
    closeAutomationBackupDialogAfterSuccess();
    notice = `${result.exportedCount} automations exported to ${result.file}.`;
  } catch (exportError) {
    automationBackupError = String(exportError);
  }

  automationBackupBusy = false;
  renderAutomationsView();
}

async function importSelectedBackupAutomations(event: SubmitEvent) {
  event.preventDefault();
  if (automationBackupBusy || automationBackupDialogMode !== "import") return;
  automationBackupError = "";

  if (!automationBackupFilePath || selectedAutomationBackupIds.size === 0) {
    automationBackupError = automationBackupFilePath
      ? "Select at least one automation to import."
      : "Choose or drop an EasyAlias automation backup first.";
    renderAutomationsView();
    return;
  }

  automationBackupBusy = true;
  renderAutomationsView();
  try {
    const result = await invokeCommand<AutomationBackupImportResult>("import_automation_backup", {
      path: automationBackupFilePath,
      selectedIds: [...selectedAutomationBackupIds],
      importedAt: nowIso()
    });
    automations = result.automations;
    closeAutomationBackupDialogAfterSuccess();
    const replacementNote = result.replacedCount
      ? ` ${result.replacedCount} existing automations replaced.`
      : "";
    notice = `${result.importedCount} automations imported.${replacementNote}`;
  } catch (backupImportError) {
    automationBackupError = String(backupImportError);
  }

  automationBackupBusy = false;
  renderAutomationsView();
}

async function stopAutomation() {
  if (!automationRun?.running || automationRun.cancelRequested) return;
  automationRun.cancelRequested = true;
  render();
  // Killing the session immediately interrupts a command that is still
  // running; without this, Stop would only take effect once that command
  // finished on its own and the loop reached its next cancellation check.
  try {
    await invokeCommand<void>("stop_automation_session", { sessionId: automationRun.sessionId });
  } catch {
    // The run loop's own cleanup will report the failure if the session is gone.
  }
}

function closeAutomationRun() {
  if (automationRun?.running) return;
  automationRun = null;
  render();
}

async function waitForAutomation(seconds: number, state: AutomationRunState) {
  const end = Date.now() + seconds * 1000;
  while (Date.now() < end) {
    if (state.cancelRequested) return false;
    await new Promise((resolve) => setTimeout(resolve, Math.min(250, end - Date.now())));
  }
  return !state.cancelRequested;
}

async function runAutomation(id: string) {
  if (automationRun?.running || automationBusy) return;
  const automation = automations.find((item) => item.id === id);
  if (!automation) return;
  if (!isTauriRuntime()) {
    error = "Automations can only run inside the EasyAlias desktop app.";
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
  automationRun = runState;
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

function automationStepLabel(step: AutomationStep) {
  if (step.kind === "wait") return `Wait ${step.seconds}s`;
  return step.command || "Command not configured";
}

function renderAutomationEditor() {
  if (!automationEditor) return "";

  return `
    <section class="modal-layer" role="presentation">
      <form class="modal-card automation-editor" id="automation-form" role="dialog" aria-modal="true" aria-labelledby="automation-editor-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">Workflow</p>
            <h2 id="automation-editor-title">${escapeHtml(automationEditor.name || "New automation")}</h2>
          </div>
          <button class="ghost-button modal-close" type="button" data-automation-action="close-editor" ${automationBusy ? "disabled" : ""}>Close</button>
        </div>

        <p class="automation-intro">Commands run from top to bottom in the same working directory. Background commands let long-running development servers start without blocking the next step.</p>
        ${automationError ? `<p class="modal-error">${escapeHtml(automationError)}</p>` : ""}

        <div class="automation-form-grid">
          <label>
            Name
            <input name="automation-name" value="${escapeHtml(automationEditor.name)}" placeholder="Development workflow" autocomplete="off" />
          </label>
          <label>
            Working Directory
            <span class="automation-path-row">
              <input name="automation-path" value="${escapeHtml(automationEditor.path)}" placeholder="~/Projects/my-app" autocomplete="off" />
              <button class="picker-button automation-folder-button" type="button" title="Choose working directory" aria-label="Choose working directory" data-automation-action="pick-folder"><i data-lucide="folder-open"></i></button>
            </span>
          </label>
          <label>
            Group <span class="automation-optional">(optional)</span>
            <span class="automation-path-row">
              <input name="automation-group" value="${escapeHtml(automationEditor.group)}" placeholder="e.g. Backend, or type a new name" autocomplete="off" maxlength="60" />
              <button class="picker-button automation-group-picker-button" type="button" title="Choose an existing group" aria-label="Choose an existing group" aria-expanded="${automationEditorGroupPickerOpen}" data-automation-action="toggle-editor-group-picker"><i data-lucide="tags"></i></button>
            </span>
            ${
              automationEditorGroupPickerOpen
                ? `<div class="automation-group-picker automation-group-picker-editor" role="menu" aria-label="Existing groups">
                    ${
                      automationGroups(automations).length
                        ? automationGroups(automations)
                            .map(
                              (name) =>
                                `<button type="button" class="automation-group-option ${automationEditor!.group.trim() === name ? "is-selected" : ""}" data-automation-action="pick-editor-group" data-group="${escapeHtml(name)}">${escapeHtml(name)}</button>`
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
            <span>${automationEditor.steps.length} configured</span>
          </div>
          <div class="automation-add-actions">
            <button class="ghost-button" type="button" data-automation-action="add-command"><i data-lucide="terminal"></i><span>Add command</span></button>
            <button class="ghost-button" type="button" data-automation-action="add-wait"><i data-lucide="clock-3"></i><span>Add wait</span></button>
          </div>
        </div>

        <div class="automation-step-list">
          ${automationEditor.steps
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
                    <button type="button" title="Move step down" aria-label="Move step ${index + 1} down" data-automation-action="move-step" data-step-index="${index}" data-offset="1" ${index === automationEditor!.steps.length - 1 ? "disabled" : ""}><i data-lucide="arrow-down"></i></button>
                    <button class="danger" type="button" title="Remove step" aria-label="Remove step ${index + 1}" data-automation-action="remove-step" data-step-index="${index}"><i data-lucide="trash-2"></i></button>
                  </div>
                </article>`
            )
            .join("")}
        </div>

        <div class="modal-actions">
          <button class="ghost-button" type="button" data-automation-action="close-editor" ${automationBusy ? "disabled" : ""}>Cancel</button>
          <button class="primary-button" type="submit" ${automationBusy ? "disabled" : ""}><i data-lucide="save"></i><span>${automationBusy ? "Saving..." : "Save automation"}</span></button>
        </div>
      </form>
    </section>`;
}

function renderAutomationRun() {
  if (!automationRun) return "";
  const automation = automations.find((item) => item.id === automationRun?.automationId);
  if (!automation) return "";
  const successful = automationRun.steps.filter((step) => step.status === "success").length;

  return `
    <section class="modal-layer" role="presentation">
      <section class="modal-card automation-runner" role="dialog" aria-modal="true" aria-labelledby="automation-run-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">${automationRun.running ? "Running" : automationRun.error ? "Run stopped" : "Completed"}</p>
            <h2 id="automation-run-title">${escapeHtml(automation.name)}</h2>
          </div>
          <span class="automation-progress">${successful} / ${automation.steps.length}</span>
        </div>
        <p class="automation-run-path"><i data-lucide="folder-open"></i><code>${escapeHtml(automation.path)}</code></p>
        ${automationRun.error ? `<p class="modal-error">${escapeHtml(automationRun.error)}</p>` : ""}
        <div class="automation-run-list">
          ${automation.steps
            .map((step, index) => {
              const result = automationRun!.steps[index];
              return `<article class="automation-run-step is-${result.status}">
                <div class="run-step-marker">${index + 1}</div>
                <div class="run-step-copy">
                  <div><strong>${step.kind === "wait" ? "Wait" : "Command"}</strong><span>${result.status}</span></div>
                  <code>${escapeHtml(automationStepLabel(step))}</code>
                  ${result.output ? `<pre>${escapeHtml(result.output)}</pre>` : ""}
                </div>
              </article>`;
            })
            .join("")}
        </div>
        <div class="modal-actions">
          ${
            automationRun.running
              ? `<button class="danger-button" type="button" data-automation-action="stop-run" ${automationRun.cancelRequested ? "disabled" : ""}><i data-lucide="circle-stop"></i><span>${automationRun.cancelRequested ? "Stopping..." : "Stop"}</span></button>`
              : `<button class="ghost-button" type="button" data-automation-action="close-run">Close</button>
                 <button class="primary-button" type="button" data-automation-action="run" data-id="${escapeHtml(automation.id)}"><i data-lucide="play"></i><span>Run again</span></button>`
          }
        </div>
      </section>
    </section>`;
}

function renderAutomationBackupDialog() {
  if (!automationBackupDialogMode) return "";

  const isExport = automationBackupDialogMode === "export";
  const allSelected =
    automationBackupCandidates.length > 0 &&
    automationBackupCandidates.every((automation) => selectedAutomationBackupIds.has(automation.id));
  const existingNames = new Set(automations.map((automation) => automation.name));
  const fileName = automationBackupFilePath.split(/[\\/]/).pop() ?? automationBackupFilePath;

  return `
    <section class="modal-layer" role="presentation">
      <form
        class="modal-card import-card backup-card"
        id="${isExport ? "automation-backup-export-form" : "automation-backup-import-form"}"
        role="dialog"
        aria-modal="true"
        aria-labelledby="automation-backup-title"
      >
        <div class="modal-title">
          <div>
            <p class="eyebrow">${isExport ? "Portable Workflows" : "Restore Workflows"}</p>
            <h2 id="automation-backup-title">${isExport ? "Export automations" : "Import automations"}</h2>
          </div>
          <button class="ghost-button modal-close" type="button" data-automation-action="close-backup" ${automationBackupBusy ? "disabled" : ""}>Close</button>
        </div>

        <p class="import-intro">
          ${
            isExport
              ? "Choose which automations to include. The versioned JSON backup keeps every path, step, behavior, and favorite state."
              : "Choose an EasyAlias automation backup or drop it below. You can review every workflow before anything is changed."
          }
        </p>

        ${automationBackupError ? `<p class="modal-error">${escapeHtml(automationBackupError)}</p>` : ""}

        ${
          isExport
            ? ""
            : `<button class="backup-drop-zone automation-backup-drop-zone" type="button" data-automation-action="choose-backup-file" ${automationBackupBusy ? "disabled" : ""}>
                <span class="backup-drop-icon" aria-hidden="true">&#8681;</span>
                <strong>${automationBackupFilePath ? escapeHtml(fileName) : "Drop an automation backup here"}</strong>
                <span>${automationBackupFilePath ? `${automationBackupCandidates.length} automations found` : "or click to choose a .json file"}</span>
              </button>`
        }

        ${
          automationBackupCandidates.length
            ? `<label class="import-select-all">
                <input type="checkbox" name="automation-backup-all" ${allSelected ? "checked" : ""} ${automationBackupBusy ? "disabled" : ""} />
                <span>Select all</span>
              </label>

              <div class="import-list" aria-label="Automations available for ${isExport ? "export" : "import"}">
                ${automationBackupCandidates
                  .map((automation) => {
                    const willReplace = !isExport && existingNames.has(automation.name);
                    const stepLabel = `${automation.steps.length} ${automation.steps.length === 1 ? "step" : "steps"}`;
                    return `
                      <label class="import-row">
                        <input
                          type="checkbox"
                          name="automation-backup-candidate"
                          value="${escapeHtml(automation.id)}"
                          ${selectedAutomationBackupIds.has(automation.id) ? "checked" : ""}
                          ${automationBackupBusy ? "disabled" : ""}
                        />
                        <span class="import-alias-copy">
                          <span class="import-alias-meta">
                            <strong>${escapeHtml(automation.name)}</strong>
                            <span class="${willReplace ? "backup-conflict" : ""}">${willReplace ? "Replaces existing" : stepLabel}</span>
                          </span>
                          <code>${escapeHtml(automation.path)}</code>
                        </span>
                      </label>`;
                  })
                  .join("")}
              </div>`
            : isExport
              ? `<p class="backup-empty">No automations are available to export.</p>`
              : ""
        }

        <p class="import-safety">
          ${
            isExport
              ? "The backup contains only the automations you select."
              : "Automations with matching names replace their current EasyAlias workflow. Unselected automations stay unchanged."
          }
        </p>

        <div class="modal-actions import-actions">
          <button class="ghost-button" type="button" data-automation-action="close-backup" ${automationBackupBusy ? "disabled" : ""}>Cancel</button>
          <button class="primary-button" type="submit" data-automation-backup-submit ${selectedAutomationBackupIds.size && !automationBackupBusy ? "" : "disabled"}>
            ${automationBackupBusy ? "Working..." : `${isExport ? "Export" : "Import"} Selected (${selectedAutomationBackupIds.size})`}
          </button>
        </div>
      </form>
    </section>`;
}

function renderAutomationTrashDialog() {
  if (!automationTrashOpen) return "";

  return `
    <section class="modal-layer" role="presentation">
      <section class="modal-card trash-card" role="dialog" aria-modal="true" aria-labelledby="automation-trash-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">Recovery</p>
            <h2 id="automation-trash-title">Automation Trash</h2>
          </div>
          <span class="import-count">${automationTrashEntries.length} deleted</span>
        </div>

        <p class="import-intro">
          Deleted automations stay here for 30 days. Restore a workflow at any time, or delete it permanently now.
        </p>

        ${automationTrashError ? `<p class="modal-error">${escapeHtml(automationTrashError)}</p>` : ""}

        ${
          automationTrashEntries.length
            ? `<div class="trash-list" aria-label="Deleted automations">
                ${automationTrashEntries
                  .map(
                    (entry) => `
                      <article class="trash-row">
                        <div class="trash-copy">
                          <strong>${escapeHtml(entry.automation.name)}</strong>
                          <span>${entry.automation.steps.length} ${entry.automation.steps.length === 1 ? "step" : "steps"}</span>
                          <code>${escapeHtml(entry.automation.path)}</code>
                          <small>Deleted ${formatDeletedDate(entry.deletedAt)} · ${trashDaysRemaining(entry.deletedAt)} days remaining</small>
                        </div>
                        <div class="trash-row-actions">
                          <button
                            class="trash-action restore"
                            type="button"
                            title="Restore ${escapeHtml(entry.automation.name)}"
                            aria-label="Restore ${escapeHtml(entry.automation.name)}"
                            data-automation-action="restore-trash"
                            data-id="${escapeHtml(entry.automation.id)}"
                            ${automationTrashBusy ? "disabled" : ""}
                          ><i data-lucide="rotate-ccw"></i></button>
                          <button
                            class="trash-action permanent"
                            type="button"
                            title="Permanently delete ${escapeHtml(entry.automation.name)}"
                            aria-label="Permanently delete ${escapeHtml(entry.automation.name)}"
                            data-automation-action="delete-trash-permanently"
                            data-id="${escapeHtml(entry.automation.id)}"
                            ${automationTrashBusy ? "disabled" : ""}
                          ><i data-lucide="trash-2"></i></button>
                        </div>
                      </article>`
                  )
                  .join("")}
              </div>`
            : `<div class="trash-empty"><strong>Automation Trash is empty</strong><span>Deleted workflows will stay recoverable here for 30 days.</span></div>`
        }

        <div class="modal-actions trash-footer-actions">
          <button class="ghost-button" type="button" data-automation-action="close-trash" ${automationTrashBusy ? "disabled" : ""}>Close</button>
          <button class="danger-button" type="button" data-automation-action="empty-trash" ${automationTrashEntries.length && !automationTrashBusy ? "" : "disabled"}>
            <i data-lucide="trash-2"></i><span>${automationTrashBusy ? "Working..." : "Empty Trash"}</span>
          </button>
        </div>
      </section>
    </section>`;
}

function matchesAutomationFilter(automation: Automation) {
  if (automationFilter.startsWith("group:")) {
    return automation.group.trim() === automationFilter.slice("group:".length);
  }

  const commandText = automation.steps
    .filter((step) => step.kind === "command")
    .map((step) => step.command)
    .join(" \n ")
    .trim()
    .toLocaleLowerCase();

  switch (automationFilter) {
    case "favorites":
      return Boolean(automation.favorite);
    case "background":
      return automation.steps.some((step) => step.kind === "command" && step.behavior === "background");
    case "git":
      return /(^|[\s;&|])git(?=\s|$)/.test(commandText);
    case "docker":
      return /(^|[\s;&|])docker(?:-compose)?(?=\s|$)/.test(commandText);
    case "build":
      return (
        /(^|[\s;&|])(?:\.\/)?(?:gradle|gradlew|mvn|mvnw|make)(?=\s|$)/.test(commandText) ||
        /(^|[\s;&|])cargo\s+build(?=\s|$)/.test(commandText) ||
        /(^|[\s;&|])(?:npm|pnpm|yarn|bun)(?:\s+run)?\s+build(?=\s|$)/.test(commandText)
      );
    case "groups":
    case "all":
    default:
      return true;
  }
}

function filterAutomations(automations: Automation[]) {
  const query = automationSearchQuery.trim().toLocaleLowerCase();

  return automations.filter(
    (automation) =>
      matchesAutomationFilter(automation) &&
      (!query ||
        automation.name.toLocaleLowerCase().includes(query) ||
        automation.path.toLocaleLowerCase().includes(query) ||
        automation.group.toLocaleLowerCase().includes(query) ||
        automation.steps.some(
          (step) => step.kind === "command" && step.command.toLocaleLowerCase().includes(query)
        ))
  );
}

function automationCountLabel(total: number, filtered: number) {
  const suffix = total === 1 ? "automation" : "automations";
  const hasActiveFilter = automationSearchQuery.trim() || automationFilter !== "all";
  return hasActiveFilter ? `${filtered} of ${total} ${suffix}` : `${total} ${suffix}`;
}

function automationOverviewCountLabel(sortedAutomations: Automation[]) {
  if (automationFilter === "groups") {
    const groupCount = automationGroups(sortedAutomations).length;
    const hasUngrouped = sortedAutomations.some((automation) => !automation.group.trim());
    const count = groupCount + (hasUngrouped ? 1 : 0);
    return `${count} ${count === 1 ? "group" : "groups"}`;
  }
  return automationCountLabel(sortedAutomations.length, filterAutomations(sortedAutomations).length);
}

// Search input stays mounted while only the result area is replaced. This
// avoids losing focus or jumping the caret while someone types quickly.
// Inline card popover for quickly assigning an automation to an existing
// group or creating a new one, without opening the full editor.
function renderAutomationGroupPicker(automation: Automation, allAutomations: Automation[]) {
  const currentGroup = automation.group.trim();
  const existingGroups = automationGroups(allAutomations);

  return `<div class="automation-group-picker" role="menu" aria-label="Assign ${escapeHtml(automation.name)} to a group">
      ${
        existingGroups.length
          ? `<div class="automation-group-picker-list">
              ${existingGroups
                .map(
                  (name) =>
                    `<button type="button" class="automation-group-option ${currentGroup === name ? "is-selected" : ""}" data-automation-action="assign-group" data-id="${escapeHtml(automation.id)}" data-group="${escapeHtml(name)}">${escapeHtml(name)}</button>`
                )
                .join("")}
            </div>`
          : `<p class="automation-group-picker-empty">No groups yet.</p>`
      }
      ${
        currentGroup
          ? `<button type="button" class="automation-group-option is-remove" data-automation-action="assign-group" data-id="${escapeHtml(automation.id)}" data-group="">No group</button>`
          : ""
      }
      <form class="automation-group-picker-create" data-automation-group-form="${escapeHtml(automation.id)}">
        <input type="text" name="automation-group-new" placeholder="New group name" maxlength="60" autocomplete="off" />
        <button class="ghost-button" type="submit">Create</button>
      </form>
    </div>`;
}

function renderAutomationGroupOverview(sortedAutomations: Automation[]) {
  const groupNames = automationGroups(sortedAutomations);
  const ungrouped = sortedAutomations.filter((automation) => !automation.group.trim());

  const cards = groupNames.map((name) => {
    const count = sortedAutomations.filter((automation) => automation.group.trim() === name).length;
    return `<button class="automation-group-card" type="button" data-automation-action="select-group" data-group="${escapeHtml(name)}">
        <i data-lucide="tag"></i>
        <span>${escapeHtml(name)}</span>
        <strong>${count} ${count === 1 ? "automation" : "automations"}</strong>
      </button>`;
  });

  if (ungrouped.length) {
    cards.push(`<button class="automation-group-card is-ungrouped" type="button" data-automation-action="select-group" data-group="">
        <i data-lucide="tag"></i>
        <span>Ungrouped</span>
        <strong>${ungrouped.length} ${ungrouped.length === 1 ? "automation" : "automations"}</strong>
      </button>`);
  }

  if (!cards.length) {
    return `<div class="automation-empty"><strong>No groups yet</strong><span>Add a group label to an automation to see it here.</span></div>`;
  }

  return `<div class="automation-group-grid">${cards.join("")}</div>`;
}

function renderAutomationResults(sortedAutomations: Automation[]) {
  if (!sortedAutomations.length) {
    return `<div class="automation-empty"><div class="automation-empty-icon"><i data-lucide="play"></i></div><strong>No automations yet</strong><span>Combine project commands and waits into a repeatable workflow.</span><button class="primary-button" type="button" data-automation-action="new"><i data-lucide="plus"></i><span>Create automation</span></button></div>`;
  }

  if (automationFilter === "groups") {
    return renderAutomationGroupOverview(sortedAutomations);
  }

  const filteredAutomations = filterAutomations(sortedAutomations);

  if (!filteredAutomations.length) {
    return `<div class="automation-empty"><strong>No matching automations</strong><span>Try another search or filter.</span></div>`;
  }

  return `<div class="automation-grid">
      ${filteredAutomations
        .map(
          (automation) => `<article class="automation-card">
              <div class="automation-card-header">
                <div class="automation-card-title">
                  <button
                    class="automation-favorite-button ${automation.favorite ? "active" : ""}"
                    type="button"
                    title="${automation.favorite ? "Remove from favorites" : "Add to favorites"}"
                    aria-label="${automation.favorite ? "Remove" : "Add"} ${escapeHtml(automation.name)} ${automation.favorite ? "from" : "to"} favorites"
                    aria-pressed="${Boolean(automation.favorite)}"
                    data-automation-action="toggle-favorite"
                    data-id="${escapeHtml(automation.id)}"
                    ${automationRun?.running ? "disabled" : ""}
                  ><i data-lucide="star"></i></button>
                  <button
                    class="automation-group-button ${automation.group.trim() ? "has-group" : ""}"
                    type="button"
                    title="${automation.group.trim() ? `Change group (currently ${escapeHtml(automation.group.trim())})` : "Assign to a group"}"
                    aria-label="${automation.group.trim() ? "Change group for" : "Assign"} ${escapeHtml(automation.name)}${automation.group.trim() ? "" : " to a group"}"
                    aria-expanded="${automationGroupPickerId === automation.id}"
                    data-automation-action="toggle-group-picker"
                    data-id="${escapeHtml(automation.id)}"
                    ${automationRun?.running ? "disabled" : ""}
                  ><i data-lucide="tags"></i></button>
                  <div>
                    <strong>${escapeHtml(automation.name)}</strong>
                    <code>${escapeHtml(automation.path)}</code>
                    ${
                      automation.group.trim()
                        ? `<button class="automation-group-chip" type="button" title="Filter by group ${escapeHtml(automation.group.trim())}" data-automation-action="select-group" data-group="${escapeHtml(automation.group.trim())}"><i data-lucide="tag"></i><span>${escapeHtml(automation.group.trim())}</span></button>`
                        : ""
                    }
                  </div>
                </div>
                <span>${automation.steps.length} ${automation.steps.length === 1 ? "step" : "steps"}</span>
              </div>
              ${automationGroupPickerId === automation.id ? renderAutomationGroupPicker(automation, sortedAutomations) : ""}
              <ol class="automation-preview-list">
                ${automation.steps
                  .slice(0, 4)
                  .map((step) => `<li><span>${step.kind === "wait" ? "Wait" : step.behavior === "background" ? "Start" : "Run"}</span><code>${escapeHtml(automationStepLabel(step))}</code></li>`)
                  .join("")}
                ${automation.steps.length > 4 ? `<li class="automation-more">+ ${automation.steps.length - 4} more</li>` : ""}
              </ol>
              <div class="automation-card-actions">
                <button class="primary-button automation-run-button" type="button" data-automation-action="run" data-id="${escapeHtml(automation.id)}" ${automationRun?.running ? "disabled" : ""}><i data-lucide="play"></i><span>Run</span></button>
                <button class="header-icon-button" type="button" title="Edit ${escapeHtml(automation.name)}" aria-label="Edit ${escapeHtml(automation.name)}" data-automation-action="edit" data-id="${escapeHtml(automation.id)}" ${automationRun?.running ? "disabled" : ""}><i data-lucide="pencil"></i></button>
                <button class="header-icon-button automation-delete-button" type="button" title="Delete ${escapeHtml(automation.name)}" aria-label="Delete ${escapeHtml(automation.name)}" data-automation-action="delete" data-id="${escapeHtml(automation.id)}" ${automationRun?.running ? "disabled" : ""}><i data-lucide="trash-2"></i></button>
              </div>
            </article>`
        )
        .join("")}
    </div>`;
}

function refreshAutomationResults() {
  const sortedAutomations = [...automations].sort(compareAutomations);
  const count = document.querySelector<HTMLElement>("[data-automation-count]");
  const results = document.querySelector<HTMLElement>("[data-automation-results]");
  const filterLabel = document.querySelector<HTMLLabelElement>(".automation-filter");

  if (count) count.textContent = automationOverviewCountLabel(sortedAutomations);
  if (filterLabel) {
    filterLabel.classList.toggle("is-active", automationFilter !== "all");
    const select = filterLabel.querySelector("select");
    if (select) select.title = `Filter: ${automationFilterLabel(automationFilter)}`;
  }
  if (!results) return;

  results.innerHTML = renderAutomationResults(sortedAutomations);
  createIcons({
    icons: { Pencil, Play, Plus, Star, Tag, Tags, Trash2 },
    attrs: { "aria-hidden": "true", width: "20", height: "20", "stroke-width": "2" }
  });
}

function renderAutomationsView() {
  const sortedAutomations = [...automations].sort(compareAutomations);
  const automationGroupNames = automationGroups(sortedAutomations);
  appElement.innerHTML = `
    <section class="shell automation-shell">
      <header class="topbar automation-topbar">
        <div>
          <p class="eyebrow">Windows Workflow Runner</p>
          <h1>Automations</h1>
        </div>
        <div class="topbar-actions">
          <button class="header-icon-button" type="button" title="Back to aliases" aria-label="Back to aliases" data-automation-action="back" ${automationRun?.running ? "disabled" : ""}><i data-lucide="arrow-left"></i></button>
          <button class="header-icon-button ${automationFilter === "groups" ? "active" : ""}" type="button" title="Group view" aria-label="Show automation groups" aria-pressed="${automationFilter === "groups"}" data-automation-action="show-groups" ${automationRun?.running ? "disabled" : ""}><i data-lucide="tags"></i></button>
          <button class="header-icon-button" type="button" title="Export automation backup" aria-label="Export automation backup" data-automation-action="open-backup-export" ${automations.length && !automationBackupBusy && !automationRun?.running ? "" : "disabled"}><i data-lucide="file-up"></i></button>
          <button class="header-icon-button" type="button" title="Import automation backup" aria-label="Import automation backup" data-automation-action="open-backup-import" ${automationBackupBusy || automationRun?.running ? "disabled" : ""}><i data-lucide="file-down"></i></button>
          <button
            class="header-icon-button trash-header-button"
            type="button"
            title="Automation Trash${automationTrashEntries.length ? ` (${automationTrashEntries.length})` : ""}"
            aria-label="Open Automation Trash${automationTrashEntries.length ? ` with ${automationTrashEntries.length} deleted automations` : ""}"
            data-automation-action="open-trash"
            ${automationTrashBusy || automationRun?.running ? "disabled" : ""}
          >
            <i data-lucide="trash-2"></i>
            ${automationTrashEntries.length ? `<span class="header-count" aria-hidden="true">${automationTrashEntries.length}</span>` : ""}
          </button>
          <button class="header-icon-button automation-create-button" type="button" title="Create automation" aria-label="Create automation" data-automation-action="new" ${automationRun?.running ? "disabled" : ""}><i data-lucide="plus"></i></button>
        </div>
      </header>

      <section class="automation-summary">
        <div><span>Automations</span><strong>${automations.length}</strong></div>
        <div><span>Execution</span><strong>Sequential</strong></div>
        <div><span>Shell</span><strong>cmd.exe</strong></div>
      </section>

      ${
        notice
          ? `<div class="message-banner notice" role="status"><span>${escapeHtml(notice)}</span><button class="message-dismiss" type="button" title="Dismiss message" aria-label="Dismiss message" data-automation-action="dismiss-message"><i data-lucide="x"></i></button></div>`
          : ""
      }
      ${
        error
          ? `<div class="message-banner error" role="alert"><span>${escapeHtml(error)}</span><button class="message-dismiss" type="button" title="Dismiss message" aria-label="Dismiss message" data-automation-action="dismiss-message"><i data-lucide="x"></i></button></div>`
          : ""
      }

      <section class="automation-overview">
        <div class="automation-overview-header">
          <div><h2>Your Automations</h2><span data-automation-count>${automationOverviewCountLabel(sortedAutomations)}</span></div>
          <button class="primary-button" type="button" data-automation-action="new" ${automationRun?.running ? "disabled" : ""}><i data-lucide="plus"></i><span>New automation</span></button>
        </div>
        <div class="automation-tools">
          <div class="automation-search" role="search">
            <i data-lucide="search"></i>
            <input
              type="search"
              name="automation-search"
              value="${escapeHtml(automationSearchQuery)}"
              placeholder="Search automations or commands"
              aria-label="Search automations by name, path, or command"
              autocomplete="off"
              ${automations.length ? "" : "disabled"}
            />
          </div>
          <label class="automation-filter ${automationFilter !== "all" ? "is-active" : ""} ${automations.length ? "" : "is-disabled"}">
            <span class="visually-hidden">Filter automations</span>
            <i data-lucide="filter"></i>
            <select
              name="automation-filter"
              aria-label="Filter automations"
              title="Filter: ${automationFilterLabel(automationFilter)}"
              ${automations.length ? "" : "disabled"}
            >
              ${Object.entries(automationFilterLabels)
                .map(
                  ([value, label]) =>
                    `<option value="${value}" ${automationFilter === value ? "selected" : ""}>${label}</option>`
                )
                .join("")}
              ${
                automationGroupNames.length
                  ? `<optgroup label="Groups">
                      ${automationGroupNames
                        .map(
                          (name) =>
                            `<option value="group:${escapeHtml(name)}" ${automationFilter === `group:${name}` ? "selected" : ""}>${escapeHtml(name)}</option>`
                        )
                        .join("")}
                    </optgroup>`
                  : ""
              }
            </select>
          </label>
        </div>
        <div class="automation-results" data-automation-results>
          ${renderAutomationResults(sortedAutomations)}
        </div>
      </section>

      ${renderAutomationEditor()}
      ${renderAutomationRun()}
      ${renderAutomationBackupDialog()}
      ${renderAutomationTrashDialog()}

      <aside class="support-banner" aria-label="Support EasyAlias"><span>Support EasyAlias development</span><a href="${sponsorUrl}" target="_blank" rel="noreferrer" data-external-link>Become a sponsor</a></aside>
      <footer class="app-footer"><a href="${repoUrl}" target="_blank" rel="noreferrer" data-external-link>© Hannes Gnann</a><span aria-hidden="true">-</span><a href="${redditUrl}" target="_blank" rel="noreferrer" data-external-link>Reddit</a><span aria-hidden="true">-</span><a href="${websiteUrl}" target="_blank" rel="noreferrer" data-external-link>Website</a></footer>
    </section>`;

  createIcons({
    icons: { ArrowDown, ArrowLeft, ArrowUp, CircleStop, Clock3, FileDown, FileUp, Filter, FolderOpen, Pencil, Play, Plus, RotateCcw, Save, Search, Star, Tag, Tags, Terminal, Trash2, X },
    attrs: { "aria-hidden": "true", width: "20", height: "20", "stroke-width": "2" }
  });
  scheduleMessageDismissal();
  bindAutomationEvents();
}

function bindAutomationEvents() {
  document.querySelector<HTMLFormElement>("#automation-form")?.addEventListener("submit", saveAutomation);
  document.querySelector<HTMLFormElement>("#automation-backup-export-form")?.addEventListener("submit", exportSelectedAutomations);
  document.querySelector<HTMLFormElement>("#automation-backup-import-form")?.addEventListener("submit", importSelectedBackupAutomations);
  document.querySelectorAll<HTMLAnchorElement>("[data-external-link]").forEach((link) => link.addEventListener("click", openExternalLink));
  document.querySelector<HTMLInputElement>('input[name="automation-name"]')?.addEventListener("input", (event) => updateAutomationEditor("name", (event.target as HTMLInputElement).value));
  document.querySelector<HTMLInputElement>('input[name="automation-path"]')?.addEventListener("input", (event) => updateAutomationEditor("path", (event.target as HTMLInputElement).value));
  document.querySelector<HTMLInputElement>('input[name="automation-group"]')?.addEventListener("input", (event) => updateAutomationEditor("group", (event.target as HTMLInputElement).value));
  document.querySelectorAll<HTMLSelectElement>('select[name="automation-step-kind"]').forEach((select) => select.addEventListener("change", () => updateAutomationStep(Number(select.dataset.stepIndex), "kind", select.value as AutomationStepKind, true)));
  document.querySelectorAll<HTMLTextAreaElement>('textarea[name="automation-step-command"]').forEach((input) => input.addEventListener("input", () => updateAutomationStep(Number(input.dataset.stepIndex), "command", input.value)));
  document.querySelectorAll<HTMLSelectElement>('select[name="automation-step-behavior"]').forEach((select) => select.addEventListener("change", () => updateAutomationStep(Number(select.dataset.stepIndex), "behavior", select.value as AutomationCommandBehavior)));
  document.querySelectorAll<HTMLInputElement>('input[name="automation-step-seconds"]').forEach((input) => input.addEventListener("input", () => updateAutomationStep(Number(input.dataset.stepIndex), "seconds", Number(input.value))));

  document.querySelector<HTMLInputElement>('input[name="automation-backup-all"]')?.addEventListener("change", (event) => {
    const checked = (event.target as HTMLInputElement).checked;
    selectedAutomationBackupIds = checked
      ? new Set(automationBackupCandidates.map((automation) => automation.id))
      : new Set();
    document.querySelectorAll<HTMLInputElement>('input[name="automation-backup-candidate"]').forEach((checkbox) => {
      checkbox.checked = checked;
    });
    syncAutomationBackupSelectionControls();
  });

  document.querySelectorAll<HTMLInputElement>('input[name="automation-backup-candidate"]').forEach((checkbox) => {
    checkbox.addEventListener("change", () => {
      if (checkbox.checked) selectedAutomationBackupIds.add(checkbox.value);
      else selectedAutomationBackupIds.delete(checkbox.value);
      syncAutomationBackupSelectionControls();
    });
  });

  if (automationBackupDialogMode) syncAutomationBackupSelectionControls();

  document.querySelector<HTMLInputElement>('input[name="automation-search"]')?.addEventListener("input", (event) => {
    automationSearchQuery = (event.target as HTMLInputElement).value;
    refreshAutomationResults();
  });

  document.querySelector<HTMLSelectElement>('select[name="automation-filter"]')?.addEventListener("change", (event) => {
    automationFilter = (event.target as HTMLSelectElement).value as AutomationFilter;
    refreshAutomationResults();
  });

  // Card actions (run/edit/delete, plus "new" in the empty state) use
  // delegation on the results container so live search/filter can replace
  // those rows without rebinding handlers or disturbing the search field.
  document.querySelector<HTMLElement>("[data-automation-results]")?.addEventListener("click", (event) => {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-automation-action]");
    if (!button) return;

    const action = button.dataset.automationAction;
    const id = button.dataset.id;
    if (action === "new") openAutomationEditor();
    if (action === "edit" && id) openAutomationEditor(id);
    if (action === "delete" && id) void deleteAutomation(id);
    if (action === "toggle-favorite" && id) void toggleAutomationFavorite(id);
    if (action === "run" && id) void runAutomation(id);
    if (action === "select-group") {
      // A full render (not refreshAutomationResults) so the filter <select>
      // picks up this group as a newly selected option and its "is-active"
      // state, exactly as if the user had chosen it from the dropdown.
      automationFilter = `group:${button.dataset.group ?? ""}`;
      render();
    }
    if (action === "toggle-group-picker" && id) toggleAutomationGroupPicker(id);
    if (action === "assign-group" && id) void assignAutomationGroup(id, button.dataset.group ?? "");
  });

  // The create-new-group mini form inside a card's group picker; submit
  // bubbles up to the same results container as the click delegation above.
  document.querySelector<HTMLElement>("[data-automation-results]")?.addEventListener("submit", (event) => {
    const form = (event.target as HTMLElement).closest<HTMLFormElement>("[data-automation-group-form]");
    if (!form) return;
    event.preventDefault();
    const id = form.dataset.automationGroupForm;
    const input = form.querySelector<HTMLInputElement>('input[name="automation-group-new"]');
    if (id && input) void assignAutomationGroup(id, input.value);
  });

  document.querySelectorAll<HTMLButtonElement>("[data-automation-action]").forEach((button) => {
    if (button.closest("[data-automation-results]")) return;
    button.addEventListener("click", () => {
      const action = button.dataset.automationAction;
      const id = button.dataset.id;
      const index = Number(button.dataset.stepIndex);
      if (action === "back") closeAutomationsView();
      if (action === "open-backup-export") openAutomationBackupExport();
      if (action === "open-backup-import") openAutomationBackupImport();
      if (action === "close-backup") closeAutomationBackupDialog();
      if (action === "choose-backup-file") void chooseAutomationBackupFile();
      if (action === "open-trash") void openAutomationTrash();
      if (action === "close-trash") closeAutomationTrash();
      if (action === "restore-trash" && id) void restoreTrashAutomation(id);
      if (action === "delete-trash-permanently" && id) void permanentlyDeleteTrashAutomation(id);
      if (action === "empty-trash") void emptyAutomationTrash();
      if (action === "new") openAutomationEditor();
      if (action === "close-editor") closeAutomationEditor();
      if (action === "pick-folder") void openPathPicker("automation", "folder");
      if (action === "add-command") addAutomationStep("command");
      if (action === "add-wait") addAutomationStep("wait");
      if (action === "move-step") moveAutomationStep(index, Number(button.dataset.offset));
      if (action === "remove-step") removeAutomationStep(index);
      if (action === "stop-run") void stopAutomation();
      if (action === "close-run") closeAutomationRun();
      if (action === "dismiss-message") dismissMessage();
      if (action === "show-groups") {
        automationFilter = "groups";
        render();
      }
      if (action === "toggle-editor-group-picker") {
        automationEditorGroupPickerOpen = !automationEditorGroupPickerOpen;
        render();
      }
      if (action === "pick-editor-group") {
        updateAutomationEditor("group", button.dataset.group ?? "");
        automationEditorGroupPickerOpen = false;
        render();
      }
    });
  });
}

// Main render function. This replaces the app HTML from state and then calls bindEvents().
// For a larger app, this would be a good candidate to split into smaller render helpers.
function render() {
  if (currentView === "automations") {
    renderAutomationsView();
    return;
  }
  if (currentView === "timed-automations") {
    renderTimedAutomationsView();
    return;
  }
  const aliases = [...appState.aliases].sort(compareAliases);
  const existingNames = new Set(aliases.map((alias) => alias.name.toLowerCase()));
  const availableSuggestions = aliasSuggestions.filter(
    (suggestion) => !existingNames.has(suggestion.name.toLowerCase())
  );
  const suggestionPageCount = Math.max(
    1,
    Math.ceil(availableSuggestions.length / suggestionPageSize)
  );
  suggestionPage = Math.min(suggestionPage, suggestionPageCount);
  const suggestionPageStart = (suggestionPage - 1) * suggestionPageSize;
  const visibleSuggestions = availableSuggestions.slice(
    suggestionPageStart,
    suggestionPageStart + suggestionPageSize
  );

  appElement.innerHTML = `
    <section class="shell">
      <header class="topbar">
        <div>
          <p class="eyebrow">Windows Alias Manager</p>
          <h1>EasyAlias</h1>
        </div>
        <div class="topbar-actions">
          <button
            class="header-icon-button"
            type="button"
            title="Timed automations"
            aria-label="Open timed automations"
            data-action="open-timed-automations"
          ><i data-lucide="clock"></i></button>
          <button
            class="header-icon-button"
            type="button"
            title="Open automations"
            aria-label="Open automations"
            data-action="open-automations"
          ><i data-lucide="play"></i></button>
          <button
            class="header-icon-button"
            type="button"
            title="Import command files"
            aria-label="Import command files"
            data-action="open-import"
            ${importBusy ? "disabled" : ""}
          ><i data-lucide="square-terminal"></i></button>
          <button
            class="header-icon-button"
            type="button"
            title="Export alias backup"
            aria-label="Export alias backup"
            data-action="open-backup-export"
            ${aliases.length && !backupBusy ? "" : "disabled"}
          ><i data-lucide="file-up"></i></button>
          <button
            class="header-icon-button"
            type="button"
            title="Import alias backup"
            aria-label="Import alias backup"
            data-action="open-backup-import"
            ${backupBusy ? "disabled" : ""}
          ><i data-lucide="file-down"></i></button>
          <button
            class="header-icon-button trash-header-button"
            type="button"
            title="Trash${trashEntries.length ? ` (${trashEntries.length})` : ""}"
            aria-label="Open Trash${trashEntries.length ? ` with ${trashEntries.length} deleted aliases` : ""}"
            data-action="open-trash"
            ${trashBusy ? "disabled" : ""}
          >
            <i data-lucide="trash-2"></i>
            ${trashEntries.length ? `<span class="header-count" aria-hidden="true">${trashEntries.length}</span>` : ""}
          </button>
        </div>
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

      ${
        notice
          ? `<div class="message-banner notice" role="status">
              <span>${escapeHtml(notice)}</span>
              <button class="message-dismiss" type="button" title="Dismiss message" aria-label="Dismiss message" data-action="dismiss-message">
                <i data-lucide="x"></i>
              </button>
            </div>`
          : ""
      }
      ${
        error
          ? `<div class="message-banner error" role="alert">
              <span>${escapeHtml(error)}</span>
              <button class="message-dismiss" type="button" title="Dismiss message" aria-label="Dismiss message" data-action="dismiss-message">
                <i data-lucide="x"></i>
              </button>
            </div>`
          : ""
      }

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
                  ? `<div id="suggestion-list">
                      <div class="suggestion-grid">
                      ${visibleSuggestions
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
                      </div>
                      ${
                        suggestionPageCount > 1
                          ? `<nav class="suggestion-pagination" aria-label="Suggestion pages">
                              <button
                                class="suggestion-page-button suggestion-page-arrow"
                                type="button"
                                title="Previous suggestion page"
                                aria-label="Previous suggestion page"
                                data-action="suggestion-page"
                                data-page="${suggestionPage - 1}"
                                ${suggestionPage === 1 ? "disabled" : ""}
                              ><i data-lucide="chevron-left"></i></button>
                              ${Array.from({ length: suggestionPageCount }, (_, index) => index + 1)
                                .map(
                                  (page) => `<button
                                    class="suggestion-page-button${page === suggestionPage ? " is-current" : ""}"
                                    type="button"
                                    aria-label="Show suggestion page ${page}"
                                    ${page === suggestionPage ? 'aria-current="page"' : ""}
                                    data-action="suggestion-page"
                                    data-page="${page}"
                                  >${page}</button>`
                                )
                                .join("")}
                              <button
                                class="suggestion-page-button suggestion-page-arrow"
                                type="button"
                                title="Next suggestion page"
                                aria-label="Next suggestion page"
                                data-action="suggestion-page"
                                data-page="${suggestionPage + 1}"
                                ${suggestionPage === suggestionPageCount ? "disabled" : ""}
                              ><i data-lucide="chevron-right"></i></button>
                            </nav>`
                          : ""
                      }
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
                        <button
                          class="favorite-button ${alias.favorite ? "active" : ""}"
                          type="button"
                          title="${alias.favorite ? "Remove from favorites" : "Add to favorites"}"
                          aria-label="${alias.favorite ? "Remove" : "Add"} ${escapeHtml(alias.name)} ${alias.favorite ? "from" : "to"} favorites"
                          aria-pressed="${Boolean(alias.favorite)}"
                          data-action="toggle-favorite"
                          data-id="${alias.id}"
                        ><i data-lucide="star"></i></button>
                        <div class="row-main">
                          <span class="alias-name">${escapeHtml(alias.name)}</span>
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

      ${renderImportModal()}
      ${renderBackupDialog()}
      ${renderTrashDialog()}
      ${renderEditModal()}

      <aside class="support-banner" aria-label="Support EasyAlias">
        <span>Support EasyAlias development</span>
        <a href="${sponsorUrl}" target="_blank" rel="noreferrer" data-external-link>
          Become a sponsor
        </a>
      </aside>

      <footer class="app-footer">
        <a href="${repoUrl}" target="_blank" rel="noreferrer" data-external-link>
          © Hannes Gnann
        </a>
        <span aria-hidden="true">-</span>
        <a href="${redditUrl}" target="_blank" rel="noreferrer" data-external-link>
          Reddit
        </a>
        <span aria-hidden="true">-</span>
        <a href="${websiteUrl}" target="_blank" rel="noreferrer" data-external-link>
          Website
        </a>
      </footer>
    </section>
  `;

  // Replace the lightweight icon placeholders after each state-driven render.
  // Importing only the icons used here keeps the production bundle tree-shakable.
  createIcons({
    icons: {
      ChevronLeft,
      ChevronRight,
      Clock,
      SquareTerminal,
      FileDown,
      FileUp,
      Play,
      RotateCcw,
      Star,
      Trash2,
      X
    },
    attrs: {
      "aria-hidden": "true",
      width: "20",
      height: "20",
      "stroke-width": "2"
    }
  });

  scheduleMessageDismissal();
  bindEvents();
}

function renderBackupDialog() {
  if (!backupDialogMode) return "";

  const isExport = backupDialogMode === "export";
  const allSelected =
    backupCandidates.length > 0 &&
    backupCandidates.every((alias) => selectedBackupIds.has(alias.id));
  const existingNames = new Set(appState.aliases.map((alias) => alias.name));
  const fileName = backupFilePath.split(/[\\/]/).pop() ?? backupFilePath;

  return `
    <section class="modal-layer" role="presentation">
      <form
        class="modal-card import-card backup-card"
        id="${isExport ? "backup-export-form" : "backup-import-form"}"
        role="dialog"
        aria-modal="true"
        aria-labelledby="backup-title"
      >
        <div class="modal-title">
          <div>
            <p class="eyebrow">${isExport ? "Portable Backup" : "Restore Backup"}</p>
            <h2 id="backup-title">${isExport ? "Export aliases" : "Import aliases"}</h2>
          </div>
          <button class="ghost-button modal-close" type="button" data-action="close-backup" ${backupBusy ? "disabled" : ""}>Close</button>
        </div>

        <p class="import-intro">
          ${
            isExport
              ? "Choose which aliases to include. The resulting JSON file can be restored with EasyAlias later."
              : "Choose an EasyAlias JSON backup or drop it below. You can review every alias before anything is changed."
          }
        </p>

        ${backupError ? `<p class="modal-error">${escapeHtml(backupError)}</p>` : ""}

        ${
          isExport
            ? ""
            : `<button class="backup-drop-zone" type="button" data-action="choose-backup-file" ${backupBusy ? "disabled" : ""}>
                <span class="backup-drop-icon" aria-hidden="true">&#8681;</span>
                <strong>${backupFilePath ? escapeHtml(fileName) : "Drop an EasyAlias backup here"}</strong>
                <span>${backupFilePath ? `${backupCandidates.length} aliases found` : "or click to choose a .json file"}</span>
              </button>`
        }

        ${
          backupCandidates.length
            ? `<label class="import-select-all">
                <input type="checkbox" name="backup-all" ${allSelected ? "checked" : ""} ${backupBusy ? "disabled" : ""} />
                <span>Select all</span>
              </label>

              <div class="import-list" aria-label="Aliases available for ${isExport ? "export" : "import"}">
                ${backupCandidates
                  .map((alias) => {
                    const willReplace = !isExport && existingNames.has(alias.name);
                    return `
                      <label class="import-row">
                        <input
                          type="checkbox"
                          name="backup-candidate"
                          value="${escapeHtml(alias.id)}"
                          ${selectedBackupIds.has(alias.id) ? "checked" : ""}
                          ${backupBusy ? "disabled" : ""}
                        />
                        <span class="import-alias-copy">
                          <span class="import-alias-meta">
                            <strong>${escapeHtml(alias.name)}</strong>
                            <span class="${willReplace ? "backup-conflict" : ""}">${willReplace ? "Replaces existing" : actionLabels[alias.action]}</span>
                          </span>
                          <code>${escapeHtml(alias.commandPreview)}</code>
                        </span>
                      </label>
                    `;
                  })
                  .join("")}
              </div>`
            : isExport
              ? `<p class="backup-empty">No aliases are available to export.</p>`
              : ""
        }

        <p class="import-safety">
          ${
            isExport
              ? "The backup contains only the aliases you select."
              : "Aliases with matching names replace their current EasyAlias entry. Unselected aliases stay unchanged."
          }
        </p>

        <div class="modal-actions import-actions">
          <button class="ghost-button" type="button" data-action="close-backup" ${backupBusy ? "disabled" : ""}>Cancel</button>
          <button class="primary-button" type="submit" data-backup-submit ${selectedBackupIds.size && !backupBusy ? "" : "disabled"}>
            ${backupBusy ? "Working..." : `${isExport ? "Export" : "Import"} Selected (${selectedBackupIds.size})`}
          </button>
        </div>
      </form>
    </section>
  `;
}

function renderTrashDialog() {
  if (!trashOpen) return "";

  return `
    <section class="modal-layer" role="presentation">
      <section class="modal-card trash-card" role="dialog" aria-modal="true" aria-labelledby="trash-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">Recovery</p>
            <h2 id="trash-title">Trash</h2>
          </div>
          <span class="import-count">${trashEntries.length} deleted</span>
        </div>

        <p class="import-intro">
          Deleted aliases stay here for 30 days. Restore an alias at any time, or delete it permanently now.
        </p>

        ${trashError ? `<p class="modal-error">${escapeHtml(trashError)}</p>` : ""}

        ${
          trashEntries.length
            ? `<div class="trash-list" aria-label="Deleted aliases">
                ${trashEntries
                  .map(
                    (entry) => `
                      <article class="trash-row">
                        <div class="trash-copy">
                          <strong>${escapeHtml(entry.alias.name)}</strong>
                          <span>${escapeHtml(actionLabels[entry.alias.action])}</span>
                          <code>${escapeHtml(entry.alias.commandPreview)}</code>
                          <small>Deleted ${formatDeletedDate(entry.deletedAt)} · ${trashDaysRemaining(entry.deletedAt)} days remaining</small>
                        </div>
                        <div class="trash-row-actions">
                          <button
                            class="trash-action restore"
                            type="button"
                            title="Restore ${escapeHtml(entry.alias.name)}"
                            aria-label="Restore ${escapeHtml(entry.alias.name)}"
                            data-action="restore-trash"
                            data-id="${escapeHtml(entry.alias.id)}"
                            ${trashBusy ? "disabled" : ""}
                          ><i data-lucide="rotate-ccw"></i></button>
                          <button
                            class="trash-action permanent"
                            type="button"
                            title="Permanently delete ${escapeHtml(entry.alias.name)}"
                            aria-label="Permanently delete ${escapeHtml(entry.alias.name)}"
                            data-action="permanently-delete-trash"
                            data-id="${escapeHtml(entry.alias.id)}"
                            ${trashBusy ? "disabled" : ""}
                          ><i data-lucide="trash-2"></i></button>
                        </div>
                      </article>
                    `
                  )
                  .join("")}
              </div>`
            : `<div class="trash-empty">
                <i data-lucide="trash-2"></i>
                <strong>Trash is empty</strong>
                <span>Deleted aliases will appear here for 30 days.</span>
              </div>`
        }

        <div class="modal-actions trash-footer-actions">
          <button class="ghost-button" type="button" data-action="close-trash" ${trashBusy ? "disabled" : ""}>Close</button>
          <button class="danger-button" type="button" data-action="empty-trash" ${trashEntries.length && !trashBusy ? "" : "disabled"}>
            <i data-lucide="trash-2"></i>
            <span>${trashBusy ? "Working..." : "Empty Trash"}</span>
          </button>
        </div>
      </section>
    </section>
  `;
}

// The same migration dialog handles both first-start discovery and a manual
// rescan from the header. The mode only changes labels and close behavior.
function renderImportModal() {
  const candidates = appState.importCandidates;
  if (!candidates.length) return "";

  const allSelected = candidates.every((candidate) => selectedImportIds.has(candidate.id));

  return `
    <section class="modal-layer" role="presentation">
      <form class="modal-card import-card" id="import-form" role="dialog" aria-modal="true" aria-labelledby="import-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">${manualImportOpen ? "Import Aliases" : "First Start"}</p>
            <h2 id="import-title">Existing command files found</h2>
          </div>
          <span class="import-count">${candidates.length} found</span>
        </div>

        <p class="import-intro">
          EasyAlias found simple .cmd or .bat aliases in user-owned PATH folders. Selected files become Custom Commands and move only after a backup is created.
        </p>

        ${importError ? `<p class="modal-error">${escapeHtml(importError)}</p>` : ""}

        <label class="import-select-all">
          <input type="checkbox" name="import-all" ${allSelected ? "checked" : ""} ${importBusy ? "disabled" : ""} />
          <span>Select all</span>
        </label>

        <div class="import-list" aria-label="Command files available for import">
          ${candidates
            .map(
              (candidate) => `
                <label class="import-row">
                  <input
                    type="checkbox"
                    name="import-candidate"
                    value="${escapeHtml(candidate.id)}"
                    ${selectedImportIds.has(candidate.id) ? "checked" : ""}
                    ${importBusy ? "disabled" : ""}
                  />
                  <span class="import-alias-copy">
                    <span class="import-alias-meta">
                      <strong>${escapeHtml(candidate.name)}</strong>
                      <span>${escapeHtml(candidate.sourceFile)}</span>
                    </span>
                    <code>${escapeHtml(candidate.command)}</code>
                  </span>
                </label>
              `
            )
            .join("")}
        </div>

        <p class="import-safety">
          Original files are copied to a timestamped <code>~/.easyalias/import-backup-*</code> folder before they are removed from the old PATH folder.
        </p>

        <div class="modal-actions import-actions">
          <button class="ghost-button" type="button" data-action="${manualImportOpen ? "close-import" : "dismiss-import"}" ${importBusy ? "disabled" : ""}>${manualImportOpen ? "Close" : "Skip Import"}</button>
          <button class="primary-button" type="submit" ${selectedImportIds.size && !importBusy ? "" : "disabled"}>
            ${importBusy ? "Working..." : `Import Selected (${selectedImportIds.size})`}
          </button>
        </div>
      </form>
    </section>
  `;
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

// Keep backup controls in sync without rebuilding the modal. Re-rendering would
// replace the scroll container and jump the user back to the beginning.
function syncBackupSelectionControls() {
  const selectedCount = backupCandidates.filter((alias) => selectedBackupIds.has(alias.id)).length;
  const selectAll = document.querySelector<HTMLInputElement>('input[name="backup-all"]');

  if (selectAll) {
    selectAll.checked = backupCandidates.length > 0 && selectedCount === backupCandidates.length;
    selectAll.indeterminate = selectedCount > 0 && selectedCount < backupCandidates.length;
  }

  const submitButton = document.querySelector<HTMLButtonElement>("[data-backup-submit]");
  if (submitButton) {
    const actionLabel = backupDialogMode === "export" ? "Export" : "Import";
    submitButton.disabled = selectedCount === 0 || backupBusy;
    submitButton.textContent = backupBusy ? "Working..." : `${actionLabel} Selected (${selectedCount})`;
  }
}

function syncAutomationBackupSelectionControls() {
  const selectedCount = automationBackupCandidates.filter((automation) =>
    selectedAutomationBackupIds.has(automation.id)
  ).length;
  const selectAll = document.querySelector<HTMLInputElement>('input[name="automation-backup-all"]');

  if (selectAll) {
    selectAll.checked =
      automationBackupCandidates.length > 0 && selectedCount === automationBackupCandidates.length;
    selectAll.indeterminate = selectedCount > 0 && selectedCount < automationBackupCandidates.length;
  }

  const submitButton = document.querySelector<HTMLButtonElement>("[data-automation-backup-submit]");
  if (submitButton) {
    const actionLabel = automationBackupDialogMode === "export" ? "Export" : "Import";
    submitButton.disabled = selectedCount === 0 || automationBackupBusy;
    submitButton.textContent = automationBackupBusy
      ? "Working..."
      : `${actionLabel} Selected (${selectedCount})`;
  }
}

// Because render() replaces the DOM, event listeners are reattached after every render.
// Small live-preview updates skip render(), so their listeners stay intact.
function bindEvents() {
  document.querySelector<HTMLFormElement>("#alias-form")?.addEventListener("submit", upsertAlias);
  document.querySelector<HTMLFormElement>("#edit-form")?.addEventListener("submit", updateAlias);
  document.querySelector<HTMLFormElement>("#import-form")?.addEventListener("submit", importSelectedCommandFiles);
  document.querySelector<HTMLFormElement>("#backup-export-form")?.addEventListener("submit", exportSelectedAliases);
  document.querySelector<HTMLFormElement>("#backup-import-form")?.addEventListener("submit", importSelectedBackupAliases);
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

  document.querySelector<HTMLInputElement>('input[name="import-all"]')?.addEventListener("change", (event) => {
    selectedImportIds = (event.target as HTMLInputElement).checked
      ? new Set(appState.importCandidates.map((candidate) => candidate.id))
      : new Set();
    render();
  });

  document.querySelectorAll<HTMLInputElement>('input[name="import-candidate"]').forEach((checkbox) => {
    checkbox.addEventListener("change", () => {
      if (checkbox.checked) selectedImportIds.add(checkbox.value);
      else selectedImportIds.delete(checkbox.value);
      render();
    });
  });

  document.querySelector<HTMLInputElement>('input[name="backup-all"]')?.addEventListener("change", (event) => {
    const checked = (event.target as HTMLInputElement).checked;
    selectedBackupIds = checked
      ? new Set(backupCandidates.map((alias) => alias.id))
      : new Set();

    document.querySelectorAll<HTMLInputElement>('input[name="backup-candidate"]').forEach((checkbox) => {
      checkbox.checked = checked;
    });
    syncBackupSelectionControls();
  });

  document.querySelectorAll<HTMLInputElement>('input[name="backup-candidate"]').forEach((checkbox) => {
    checkbox.addEventListener("change", () => {
      if (checkbox.checked) {
        selectedBackupIds.add(checkbox.value);
      } else {
        selectedBackupIds.delete(checkbox.value);
      }
      syncBackupSelectionControls();
    });
  });

  if (backupDialogMode) {
    syncBackupSelectionControls();
  }

  document.querySelectorAll<HTMLButtonElement>("[data-action]").forEach((button) => {
    button.addEventListener("click", () => {
      const action = button.dataset.action;
      const id = button.dataset.id;

      if (action === "open-timed-automations") openTimedAutomationsView();
      if (action === "open-automations") openAutomationsView();
      if (action === "open-import") void openCommandFileImport();
      if (action === "open-backup-export") openBackupExport();
      if (action === "open-backup-import") openBackupImport();
      if (action === "open-trash") void openTrash();
      if (action === "close-trash") closeTrash();
      if (action === "restore-trash" && id) void restoreTrashAlias(id);
      if (action === "permanently-delete-trash" && id) void permanentlyDeleteTrashAlias(id);
      if (action === "empty-trash") void emptyTrash();
      if (action === "dismiss-message") dismissMessage();
      if (action === "toggle-favorite" && id) void toggleFavorite(id);
      if (action === "close-backup") closeBackupDialog();
      if (action === "choose-backup-file") void chooseBackupFile();
      if (action === "close-import") closeManualImport();
      if (action === "dismiss-import") void dismissCommandFileImport();
      if (action === "edit" && id) openEditModal(id);
      if (action === "close-edit") closeEditModal();
      if (action === "toggle-suggestions") toggleSuggestions();
      if (action === "suggestion-page") {
        showSuggestionPage(Number(button.dataset.page));
      }
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

// Tauri reports native file drops even though the HTML drop event does not
// contain a browser File object. Only drops while the backup dialog is open are
// consumed; dropping multiple files produces a clear validation message.
async function bindNativeBackupDrop() {
  if (!isTauriRuntime()) return;

  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    await getCurrentWebview().onDragDropEvent((event) => {
      const isAliasImport = backupDialogMode === "import";
      const isAutomationImport = automationBackupDialogMode === "import";
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
          automationBackupError = "Drop exactly one EasyAlias automation JSON backup.";
          renderAutomationsView();
        } else {
          backupError = "Drop exactly one EasyAlias JSON backup.";
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

// Initial app boot.
void bindNativeBackupDrop();
void loadState();
