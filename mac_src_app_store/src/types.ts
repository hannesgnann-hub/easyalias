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
// commandPreview is stored too, so the backend can write aliases.zsh without
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

// The backend exposes only conservative, single-line aliases as import choices.
// The source file and line number let users locate a command before confirming it.
export type ShellAliasCandidate = {
  id: string;
  name: string;
  command: string;
  lineNumber: number;
  sourceFile: string;
};

// AppState mirrors what the Rust backend returns to the frontend.
// The file paths are included so the UI can show where EasyAlias stores data.
export type AppState = {
  aliases: AliasEntry[];
  configFile: string;
  aliasTarget: string;
  homePath: string | null;
  homeConnected: boolean;
  shellConfigFile: string;
  managedBlockPresent: boolean;
  connectionError: string | null;
  importCandidates: ShellAliasCandidate[];
};

export type ImportResult = {
  state: AppState;
  importedCount: number;
  backupFile: string;
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

// Suggestions use the same fields as the create form and add display metadata.
// Keeping them structured means direct saves and previews use the normal app logic.
export type AliasSuggestion = AliasForm & {
  id: string;
  description: string;
};

export type PickerTarget = "create" | "edit";

export type PickerKind = "file" | "folder";

export type BackupDialogMode = "export" | "import";
