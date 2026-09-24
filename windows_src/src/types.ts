// Shared TypeScript types for aliases, automations, settings and the backend API.

// Actions are the high-level choices shown in the dropdown.
// The selected action decides how the final shell command is generated.
export type AliasAction =
  | "navigate"
  | "open"
  | "execute"
  | "compile_gradle"
  | "compile_maven"
  | "custom";

// This is the canonical alias data shape used by the UI and persisted as JSON.
// commandPreview is stored too, so the backend can write .cmd files without
// needing to duplicate all frontend command-generation rules.
export type AliasEntry = {
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

export type CommandFileCandidate = {
  id: string;
  name: string;
  command: string;
  sourceFile: string;
};

// AppState mirrors what the Rust backend returns to the frontend.
// The file paths are included so the UI can show where EasyAlias stores data.
export type AppState = {
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

export type ImportResult = {
  state: AppState;
  importedCount: number;
  backupDir: string;
  warning?: string;
};

export type BackupExportResult = {
  file: string;
  exportedCount: number;
};

export type BackupImportResult = {
  state: AppState;
  importedCount: number;
  replacedCount: number;
};

export type TrashEntry = {
  alias: AliasEntry;
  deletedAt: number;
};

export type TrashMutationResult = {
  state: AppState;
  trash: TrashEntry[];
};

// AliasForm is the temporary state for either the create form or the edit modal.
// It is intentionally close to AliasEntry but does not include timestamps.
export type AliasForm = {
  id?: string;
  name: string;
  path: string;
  action: AliasAction;
  customCommand: string;
};

// Suggestions share the regular form fields so command generation and direct
// persistence follow exactly the same path as manually created shortcuts.
export type AliasSuggestion = AliasForm & {
  id: string;
  description: string;
};

export type PickerTarget = "create" | "edit" | "automation";

export type PickerKind = "file" | "folder";

export type BackupDialogMode = "export" | "import";

export type AppView = "aliases" | "automations" | "settings";

export type ThemePreference = "system" | "light" | "dark";

export type HotkeyBehavior = "window" | "background";

export type AppSettings = {
  theme: ThemePreference;
  hotkeyBehavior: HotkeyBehavior;
  showSuggestions: boolean;
  autostart: boolean;
};

export type AutomationStepKind = "command" | "wait";

export type AutomationCommandBehavior = "wait" | "background";

export type AutomationRunStepStatus = "pending" | "running" | "success" | "error" | "skipped";

export type AutomationStep = {
  id: string;
  kind: AutomationStepKind;
  command: string;
  seconds: number;
  behavior: AutomationCommandBehavior;
};

export type Automation = {
  id: string;
  name: string;
  path: string;
  steps: AutomationStep[];
  favorite: boolean;
  // Free-text label used to organize automations. Empty means ungrouped.
  group: string;
  // Optional global keyboard shortcut (Tauri accelerator string, e.g.
  // "CmdOrCtrl+Shift+L"). null/undefined means no shortcut is assigned.
  hotkey?: string | null;
  createdAt: string;
  updatedAt: string;
};

export type AutomationTrashEntry = {
  automation: Automation;
  deletedAt: number;
};

export type AutomationTrashMutationResult = {
  automations: Automation[];
  trash: AutomationTrashEntry[];
};

export type AutomationBackupImportResult = {
  automations: Automation[];
  importedCount: number;
  replacedCount: number;
};

export type AutomationCommandResult = {
  exitCode: number | null;
  stdout: string;
  stderr: string;
  processId: number | null;
};

export type AutomationRunStep = {
  status: AutomationRunStepStatus;
  output: string;
};

export type AutomationRunState = {
  automationId: string;
  sessionId: string;
  running: boolean;
  cancelRequested: boolean;
  currentStep: number;
  error: string;
  steps: AutomationRunStep[];
};

// Schedules an existing Automation to run through the OS's own scheduler
// (Windows Task Scheduler), so it fires even while EasyAlias itself is not
// running. `triggerKind` is "clock" (a fixed "HH:MM" in `time`), or
// "sunrise"/"sunset" (that day's actual event for the configured region,
// recomputed daily - `time` is unused then). `days` uses lowercase
// three-letter abbreviations; an empty array means every day.
export type TimedAutomationTriggerKind = "clock" | "sunrise" | "sunset";

export type TimedAutomation = {
  id: string;
  automationId: string;
  triggerKind: TimedAutomationTriggerKind;
  time: string;
  days: string[];
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
  lastRunAt: number | null;
  lastRunStatus: "success" | "error" | null;
  lastRunOutput: string | null;
};

// A small fixed list of region keys (each mapped server-side to a
// representative city's coordinates) used to approximate sunrise/sunset -
// not exact per-address location, just close enough that "sunrise" fires
// within a few minutes of the real one.
export type SunRegionOption = {
  value: string;
  label: string;
};

// A single global setting (not per-automation - the user has one physical
// location) that every sunrise/sunset timed automation shares.
export type SunLocationSetting = {
  region: string;
};

// Filters are inferred from step commands (same patterns as the alias
// filter, minus favorites, which this automation model does not have yet)
// plus background steps and groups. "groups" switches the results area to
// a browsable group overview instead of filtering the automation list
// directly; a `group:<name>` value (picked from a group card or the
// dropdown) filters to that one group, where `group:` with an empty name
// means "ungrouped".
export type AutomationStaticFilter = "all" | "favorites" | "background" | "git" | "docker" | "build" | "groups";

export type AutomationFilter = AutomationStaticFilter | `group:${string}`;

// Guided tutorial overlay. `tutorialOpen` shows the modal; a null topic shows
// the three-way picker, a topic shows that walkthrough at `tutorialStep`.
export type TutorialTopic = "aliases" | "automations" | "support";

// ---- Guided tutorial ----

export type TutorialStepContent = { title: string; body: string; image?: string };

export type TutorialContent = {
  label: string;
  icon: string;
  blurb: string;
  steps: TutorialStepContent[];
};
