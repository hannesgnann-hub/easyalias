// Static labels, option lists, defaults and external links.

import type { AliasAction, AliasForm } from "./types";

export const actionLabels: Record<AliasAction, string> = {
  navigate: "Go to Folder",
  open: "Open",
  execute: "Run",
  compile_gradle: "Gradle Build",
  compile_maven: "Maven Build",
  custom: "Custom Command"
};

export const emptyForm: AliasForm = {
  name: "",
  path: "",
  action: "navigate",
  customCommand: ""
};

export const trashRetentionSeconds = 30 * 24 * 60 * 60;

export const repoUrl = "https://github.com/hannesgnann-hub/easyalias";

export const redditUrl = "https://www.reddit.com/r/easyalias/";

export const websiteUrl = "https://easyalias.org";
