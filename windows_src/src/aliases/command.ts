// Turns an alias form into the generated shell command.

import type { AliasEntry, AliasForm } from "../types";

// Converts a user-entered path into a safe cmd.exe command argument.
//
// Examples:
//   ~/Desktop/app      -> "%USERPROFILE%\Desktop\app"
//   C:\Tools\run.bat   -> "C:\Tools\run.bat"
//
// The generated command is written into a .cmd file. That means we intentionally
// use cmd.exe syntax here, not PowerShell syntax.
export function cmdPath(path: string) {
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
export function escapeCmdDoubleQuoted(value: string) {
  return value.replace(/%/g, "%%").replace(/"/g, '""');
}

// Converts the selected action + path/custom command into the shell command
// that will later be written into a .cmd file.
export function buildCommandPreview(entry: Pick<AliasEntry, "path" | "action" | "customCommand">) {
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
