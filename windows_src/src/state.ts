import { emptyForm } from "./constants";
import type {
  AliasEntry,
  AliasForm,
  AppSettings,
  AppState,
  AppView,
  Automation,
  AutomationFilter,
  AutomationRunState,
  AutomationTrashEntry,
  BackupDialogMode,
  SunLocationSetting,
  SunRegionOption,
  TimedAutomation,
  TrashEntry,
  TutorialTopic
} from "./types";

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
  currentView: AppView;
  automations: Automation[];
  automationEditor: Automation | null;
  automationBusy: boolean;
  automationError: string;
  automationRun: AutomationRunState | null;
  automationSearchQuery: string;
  automationFilter: AutomationFilter;
  automationGroupPickerId: string | null;
  automationEditorGroupPickerOpen: boolean;
  scheduleEditorAutomationId: string | null;
  automationBackupDialogMode: BackupDialogMode | null;
  automationBackupCandidates: Automation[];
  selectedAutomationBackupIds: Set<string>;
  automationBackupFilePath: string;
  automationBackupBusy: boolean;
  automationBackupError: string;
  automationTrashEntries: AutomationTrashEntry[];
  automationTrashOpen: boolean;
  automationTrashBusy: boolean;
  automationTrashError: string;
  timedAutomations: TimedAutomation[];
  timedAutomationEditor: TimedAutomation | null;
  timedAutomationBusy: boolean;
  timedAutomationError: string;
  sunRegionOptions: SunRegionOption[];
  sunLocation: SunLocationSetting;
  appSettings: AppSettings;
  settingsBusy: boolean;
  settingsError: string;
  settingsReturnView: Exclude<AppView, "settings">;
  hotkeyEditorAutomationId: string | null;
  hotkeyDraft: string | null;
  hotkeyCaptureError: string;
  tutorialOpen: boolean;
  tutorialTopic: TutorialTopic | null;
  tutorialStep: number;
};

export const state: UiState = {
  // Global UI state. For this prototype we keep state in module-level variables
  // and re-render the app when larger UI structure changes.
  appState: {
  aliases: [],
  configFile: "~/.easyalias/config.json",
  commandDir: "~/.easyalias/bin",
  pathEntry: "~/.easyalias/bin",
  pathConfigured: false,
  importCandidates: []
},
  form: { ...emptyForm },
  editForm: null,
  editingId: null,

  // Suggestions remain out of the main workflow until the user expands them.
  suggestionsExpanded: false,
  suggestionPage: 1,
  selectedImportIds: new Set<string>(),
  importBusy: false,

  // Manual imports share the first-start modal but close without writing the
  // one-time import marker.
  manualImportOpen: false,
  notice: "",
  error: "",
  messageDismissTimer: null,
  scheduledMessageKey: "",
  editError: "",
  importError: "",

  // Backup import/export has its own modal state so it never interferes with the
  // first-start legacy command-file migration flow above.
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

  // Automations have their own view and storage. The editor keeps a detached
  // draft so cancelling never mutates a saved workflow.
  currentView: "aliases",
  automations: [],
  automationEditor: null,
  automationBusy: false,
  automationError: "",
  automationRun: null,
  automationSearchQuery: "",
  automationFilter: "all",

  // Only one card's quick group-picker is open at a time; the editor's picker
  // is separate since it lives in its own modal.
  automationGroupPickerId: null,
  automationEditorGroupPickerOpen: false,

  // Only one card's quick schedule-picker is open at a time, same as the
  // group picker above.
  scheduleEditorAutomationId: null,

  // Automation backups deliberately use their own state and file format. This
  // prevents a workflow backup from being mistaken for an alias backup while
  // retaining the same selective export/import experience.
  automationBackupDialogMode: null,
  automationBackupCandidates: [],
  selectedAutomationBackupIds: new Set<string>(),
  automationBackupFilePath: "",
  automationBackupBusy: false,
  automationBackupError: "",

  // Deleted workflows use the same 30-day recovery model as aliases, but live
  // in their own file so alias and automation data can never be mixed.
  automationTrashEntries: [],
  automationTrashOpen: false,
  automationTrashBusy: false,
  automationTrashError: "",

  // Timed automations are their own small list within the placeholder view.
  // The editor keeps a detached draft, same pattern as the automation editor.
  timedAutomations: [],
  timedAutomationEditor: null,
  timedAutomationBusy: false,
  timedAutomationError: "",

  // Sunrise/sunset region list and the currently configured region are both
  // global (shared by every sunrise/sunset timed automation), loaded once at
  // startup and editable from inside the schedule modal.
  sunRegionOptions: [],
  sunLocation: { region: "" },
  appSettings: {
  theme: "system",
  hotkeyBehavior: "window",
  showSuggestions: true,
  autostart: false
},
  settingsBusy: false,
  settingsError: "",

  // Where the "back" button in Settings returns to (whichever view opened it).
  settingsReturnView: "aliases",

  // Only one automation card's hotkey-capture popover is open at a time.
  hotkeyEditorAutomationId: null,
  hotkeyDraft: null,
  hotkeyCaptureError: "",
  tutorialOpen: false,
  tutorialTopic: null,
  tutorialStep: 0,
};

// Nine cards fill the three-column layout and keep every page the same height.
export const suggestionPageSize = 9;
