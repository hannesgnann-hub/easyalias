// Automation sorting, filtering and groups.

import { automationFilterLabels } from "../constants";
import { state } from "../state";
import type { Automation, AutomationFilter, AutomationStaticFilter } from "../types";

export function automationFilterLabel(filter: AutomationFilter): string {
  if (filter.startsWith("group:")) {
    const name = filter.slice("group:".length);
    return name ? `Group: ${name}` : "Ungrouped";
  }
  return automationFilterLabels[filter as AutomationStaticFilter];
}

export function automationGroups(automations: Automation[]) {
  const names = new Set<string>();
  for (const automation of automations) {
    const trimmed = automation.group.trim();
    if (trimmed) names.add(trimmed);
  }
  return [...names].sort((left, right) => left.localeCompare(right));
}

export function compareAutomations(left: Automation, right: Automation) {
  const favoriteDifference = Number(Boolean(right.favorite)) - Number(Boolean(left.favorite));
  return favoriteDifference || left.name.localeCompare(right.name);
}

export function matchesAutomationFilter(automation: Automation) {
  if (state.automationFilter.startsWith("group:")) {
    return automation.group.trim() === state.automationFilter.slice("group:".length);
  }

  const commandText = automation.steps
    .filter((step) => step.kind === "command")
    .map((step) => step.command)
    .join(" \n ")
    .trim()
    .toLocaleLowerCase();

  switch (state.automationFilter) {
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

export function filterAutomations(automations: Automation[]) {
  const query = state.automationSearchQuery.trim().toLocaleLowerCase();

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

export function automationCountLabel(total: number, filtered: number) {
  const suffix = total === 1 ? "automation" : "automations";
  const hasActiveFilter = state.automationSearchQuery.trim() || state.automationFilter !== "all";
  return hasActiveFilter ? `${filtered} of ${total} ${suffix}` : `${total} ${suffix}`;
}

export function automationOverviewCountLabel(sortedAutomations: Automation[]) {
  if (state.automationFilter === "groups") {
    const groupCount = automationGroups(sortedAutomations).length;
    const hasUngrouped = sortedAutomations.some((automation) => !automation.group.trim());
    const count = groupCount + (hasUngrouped ? 1 : 0);
    return `${count} ${count === 1 ? "group" : "groups"}`;
  }
  return automationCountLabel(sortedAutomations.length, filterAutomations(sortedAutomations).length);
}
