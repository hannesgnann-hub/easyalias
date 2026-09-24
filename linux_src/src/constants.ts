// Static labels, option lists, defaults and external links.

import type { AliasAction, AliasForm, AutomationStaticFilter, SunRegionOption } from "./types";

export const weekdayOrder = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"] as const;

export const weekdayLabels: Record<(typeof weekdayOrder)[number], string> = {
  mon: "Mon",
  tue: "Tue",
  wed: "Wed",
  thu: "Thu",
  fri: "Fri",
  sat: "Sat",
  sun: "Sun"
};

// Browser-preview fallback for list_sun_regions - mirrors the backend's
// SUN_REGIONS table (Rust is still the source of truth in the desktop app).
export const SUN_REGION_OPTIONS: SunRegionOption[] = [
  { value: "us-pacific", label: "US Pacific (Los Angeles)" },
  { value: "us-mountain", label: "US Mountain (Denver)" },
  { value: "us-central", label: "US Central (Chicago)" },
  { value: "us-eastern", label: "US Eastern (New York)" },
  { value: "uk-ireland", label: "UK & Ireland (London)" },
  { value: "eu-west", label: "EU West (Paris)" },
  { value: "eu-central", label: "EU Central (Berlin)" },
  { value: "eu-east", label: "EU East (Kyiv)" },
  { value: "asia-east", label: "East Asia (Tokyo)" },
  { value: "asia-south", label: "South Asia (Delhi)" },
  { value: "australia", label: "Australia (Sydney)" }
];

export const actionLabels: Record<AliasAction, string> = {
  navigate: "Go to Folder",
  open: "Open",
  execute: "Run",
  compile_gradle: "Gradle Build",
  compile_maven: "Maven Build",
  custom: "Custom Command"
};

export const automationFilterLabels: Record<AutomationStaticFilter, string> = {
  all: "All automations",
  favorites: "Favorites",
  background: "Background",
  git: "Git",
  docker: "Docker",
  build: "Build",
  groups: "Group view"
};

export const emptyForm: AliasForm = {
  name: "",
  path: "",
  action: "navigate",
  customCommand: ""
};

export const trashRetentionSeconds = 30 * 24 * 60 * 60;

export const settingsStorageKey = "easyalias-settings";

export const repoUrl = "https://github.com/hannesgnann-hub/easyalias";

export const redditUrl = "https://www.reddit.com/r/easyalias/";

export const websiteUrl = "https://easyalias.org";

export const sponsorUrl = "https://github.com/sponsors/hannesgnann-hub";
