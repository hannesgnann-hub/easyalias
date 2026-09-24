import { emptyForm } from "./constants";
import type { AliasEntry, AliasForm, AppState, BackupDialogMode, TrashEntry } from "./types";

// Global UI state. Every piece of mutable, module-wide state lives on this
// one object so all modules share (and can reassign) the same values.
type UiState = {
  appState: AppState;
  form: AliasForm;
  editForm: AliasForm | null;
  editingId: string | null;
  suggestionsExpanded: boolean;
  suggestionPage: number;
  selectedImportIds: Set<string>;
  importBusy: boolean;
  manualImportOpen: boolean;
  notice: string;
  error: string;
  messageDismissTimer: ReturnType<typeof setTimeout> | null;
  scheduledMessageKey: string;
  editError: string;
  importError: string;
  backupDialogMode: BackupDialogMode | null;
  backupCandidates: AliasEntry[];
  selectedBackupIds: Set<string>;
  backupFilePath: string;
  backupBusy: boolean;
  backupError: string;
  trashEntries: TrashEntry[];
  trashOpen: boolean;
  trashBusy: boolean;
  trashError: string;
};

export const state: UiState = {
  // Global UI state. For this prototype we keep state in module-level variables
  // and re-render the app when larger UI structure changes.
  appState: {
  aliases: [],
  configFile: "App Sandbox container/config.json",
  aliasTarget: "Choose your Home folder to connect shell files",
  homePath: null,
  homeConnected: false,
  shellConfigFile: "~/.zshrc, ~/.bash_profile and ~/.bashrc",
  managedBlockPresent: false,
  connectionError: null,
  importCandidates: []
},
  form: { ...emptyForm },
  editForm: null,
  editingId: null,

  // Suggestions start collapsed so they do not compete with the main workflow.
  // The state remains stable across normal renders until the user toggles it.
  suggestionsExpanded: false,
  suggestionPage: 1,

  // Import candidates are selected by default so the common first-run path is a
  // review followed by one confirmation, while every alias can still be excluded.
  selectedImportIds: new Set<string>(),
  importBusy: false,

  // A manual import can be closed without changing the first-start marker.
  // This also lets the shared modal adjust its heading and secondary action.
  manualImportOpen: false,
  notice: "",
  error: "",
  messageDismissTimer: null,
  scheduledMessageKey: "",
  editError: "",
  importError: "",

  // Backup import/export has its own modal state so it never interferes with the
  // first-start shell-file migration flow above.
  backupDialogMode: null,
  backupCandidates: [],
  selectedBackupIds: new Set<string>(),
  backupFilePath: "",
  backupBusy: false,
  backupError: "",

  // Deleted aliases remain recoverable for 30 days. Native builds persist these
  // entries in ~/.easyalias/trash.json; browser preview mirrors them locally.
  trashEntries: [],
  trashOpen: false,
  trashBusy: false,
  trashError: "",
};

// Nine cards fill the three-column layout and keep every page the same height.
export const suggestionPageSize = 9;
