// Built-in workflow templates for Windows. Templates open in the regular
// editor so their directory and commands can be reviewed before saving.

import { createId, nowIso } from "../platform";
import { render } from "../render";
import { state } from "../state";
import type { AutomationStep } from "../types";

type AutomationTemplateStep = Omit<AutomationStep, "id">;
export type AutomationSuggestion = { id: string; name: string; description: string; path: string; group: string; steps: AutomationTemplateStep[] };

const command = (value: string, behavior: AutomationStep["behavior"] = "wait"): AutomationTemplateStep => ({ kind: "command", command: value, seconds: 0, behavior });
const wait = (seconds: number): AutomationTemplateStep => ({ kind: "wait", command: "", seconds, behavior: "wait" });

export const automationSuggestions: AutomationSuggestion[] = [
  { id: "docker-rebuild", name: "Docker rebuild", description: "Recreate a Compose stack from a clean build", path: "~/Projects", group: "Docker", steps: [command("docker compose down"), command("docker compose up -d --build")] },
  { id: "node-development", name: "Node development", description: "Install dependencies and start the dev server", path: "~/Projects", group: "Node", steps: [command("npm install"), command("npm run dev", "background")] },
  { id: "node-quality", name: "Node quality check", description: "Run linting, tests, and a production build", path: "~/Projects", group: "Node", steps: [command("npm run lint"), command("npm test"), command("npm run build")] },
  { id: "git-sync", name: "Git sync", description: "Rebase local work and show the resulting status", path: "~/Projects", group: "Git", steps: [command("git pull --rebase"), command("git status --short --branch")] },
  { id: "maven-package", name: "Maven package", description: "Run a clean Maven production package", path: "~/Projects", group: "Java", steps: [command("mvn clean package")] },
  { id: "gradle-check", name: "Gradle verification", description: "Clean, test, and build a Gradle project", path: "~/Projects", group: "Java", steps: [command("call gradlew.bat clean test build")] },
  { id: "python-setup", name: "Python setup", description: "Create a virtual environment and install requirements", path: "~/Projects", group: "Python", steps: [command("py -m venv .venv"), command("call .venv\\Scripts\\activate.bat && python -m pip install -r requirements.txt")] },
  { id: "service-restart", name: "Service restart", description: "Restart a Compose service and pause before checking it", path: "~/Projects", group: "Docker", steps: [command("docker compose restart"), wait(5), command("docker compose ps")] }
];

export function availableAutomationSuggestions() { const names = new Set(state.automations.map((item) => item.name.trim().toLowerCase())); return automationSuggestions.filter((item) => !names.has(item.name.toLowerCase())); }
export function toggleAutomationSuggestions() { state.automationSuggestionsExpanded = !state.automationSuggestionsExpanded; render(); }
export function showAutomationSuggestionPage(page: number) { if (!Number.isFinite(page)) return; state.automationSuggestionPage = Math.max(1, Math.floor(page)); render(); }
export function useAutomationSuggestion(id: string) {
  const suggestion = automationSuggestions.find((item) => item.id === id);
  if (!suggestion) return;
  if (state.automations.some((item) => item.name.trim().toLowerCase() === suggestion.name.toLowerCase())) { state.error = `Automation \"${suggestion.name}\" already exists.`; render(); return; }
  const timestamp = nowIso();
  state.automationEditor = { id: createId(), name: suggestion.name, path: suggestion.path, steps: suggestion.steps.map((step) => ({ ...step, id: createId() })), favorite: false, group: suggestion.group, createdAt: timestamp, updatedAt: timestamp };
  state.automationError = "";
  state.automationEditorGroupPickerOpen = false;
  render();
}
