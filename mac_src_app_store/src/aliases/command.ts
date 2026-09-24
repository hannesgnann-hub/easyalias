// Turns an alias form into the generated shell command.

import type { AliasEntry, AliasForm } from "../types";

// Converts a user-entered path into a safe zsh command argument.
// "~/" is expanded to "$HOME/" so generated aliases keep working reliably.
export function shellPath(path: string) {
  const trimmed = path.trim();
  if (!trimmed) return "";

  if (trimmed === "~") return '"$HOME"';
  if (trimmed.startsWith("~/")) {
    return `"$HOME/${escapeDoubleQuoted(trimmed.slice(2))}"`;
  }

  return `"${escapeDoubleQuoted(trimmed)}"`;
}

// Escape characters that can break a double-quoted zsh string.
export function escapeDoubleQuoted(value: string) {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/`/g, "\\`").replace(/\$/g, "\\$");
}

// Converts the selected action + path/custom command into the shell command
// that will later be written into aliases.zsh.
export function buildCommandPreview(entry: Pick<AliasEntry, "path" | "action" | "customCommand">) {
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
export function validateAlias(formValue: AliasForm) {
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
