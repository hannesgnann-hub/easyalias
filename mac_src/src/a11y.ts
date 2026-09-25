// Accessibility plumbing shared by every view.
//
// The UI re-renders by replacing innerHTML, which throws away the focused
// element and silently re-inserts status banners. `replaceAppHtml` wraps every
// full render so that:
//   - keyboard focus survives the re-render (same control, same caret),
//   - a newly opened dialog receives focus, and a closed one hands it back
//     to the control that opened it,
//   - everything behind an open dialog is `inert`, and Tab stays inside it,
//   - Escape closes the open popover or dialog,
//   - messages are announced once through persistent live regions instead of
//     being re-announced on every render.

import { appElement } from "./dom";

type FocusDescriptor = {
  selector: string;
  fallbackSelector: string | null;
  selectionStart: number | null;
  selectionEnd: number | null;
};

// Attributes that identify "the same control" across two renders.
const IDENTITY_ATTRIBUTES = [
  "id",
  "name",
  "data-action",
  "data-automation-action",
  "data-settings-action",
  "data-timed-action",
  "data-tutorial-action",
  "data-id",
  "data-suggestion-id",
  "data-page",
  "data-value",
  "data-step-index",
  "data-offset",
  "data-kind",
  "data-target",
  "data-group",
  "data-day",
  "data-trigger-kind",
  "data-topic",
  "data-step",
  "data-setting"
];

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

// Controls that close a dialog. Deliberately excludes decisions such as the
// first-run "Don't import" button - Escape must never make a choice.
const DIALOG_CLOSE =
  '[data-action="close-edit"], [data-action="close-backup"], [data-action="close-trash"], [data-action="close-import"], ' +
  '[data-automation-action="close-editor"], [data-automation-action="close-backup"], [data-automation-action="close-trash"], ' +
  '[data-automation-action="close-run"], [data-timed-action="close"], [data-tutorial-action="close"]';

// Open inline popovers and the control that toggles each one closed.
const POPOVER_TOGGLES =
  '[data-automation-action="toggle-group-picker"][aria-expanded="true"], ' +
  '[data-automation-action="toggle-hotkey-picker"][aria-expanded="true"], ' +
  '[data-automation-action="toggle-editor-group-picker"][aria-expanded="true"]';

let lastDialogKey: string | null = null;
let focusBeforeDialog: FocusDescriptor | null = null;
let announcedTexts = new Set<string>();
let firstRender = true;

function isVisible(element: HTMLElement) {
  return !!(element.offsetWidth || element.offsetHeight || element.getClientRects().length);
}

function selectorFor(element: Element, attributes: string[]) {
  let selector = element.tagName.toLowerCase();
  for (const attribute of attributes) {
    const value = element.getAttribute(attribute);
    if (value !== null) selector += `[${attribute}="${CSS.escape(value)}"]`;
  }
  return selector;
}

function describeFocus(): FocusDescriptor | null {
  const active = document.activeElement as HTMLElement | null;
  if (!active || active === document.body || !appElement.contains(active)) return null;
  const present = IDENTITY_ATTRIBUTES.filter((attribute) => active.hasAttribute(attribute));
  if (!present.length) return null;
  const primary = present.find((attribute) => attribute === "id" || attribute === "name" || attribute.endsWith("action"));
  const field = active as HTMLInputElement;
  let selectionStart: number | null = null;
  let selectionEnd: number | null = null;
  try {
    selectionStart = typeof field.selectionStart === "number" ? field.selectionStart : null;
    selectionEnd = typeof field.selectionEnd === "number" ? field.selectionEnd : null;
  } catch {
    // Some input types (e.g. search in older WebKit) throw on selection access.
  }
  return {
    selector: selectorFor(active, present),
    fallbackSelector: primary ? selectorFor(active, [primary]) : null,
    selectionStart,
    selectionEnd
  };
}

function restoreFocus(descriptor: FocusDescriptor | null, within: ParentNode = appElement): boolean {
  if (!descriptor) return false;
  const candidates = [descriptor.selector, descriptor.fallbackSelector].filter(Boolean) as string[];
  for (const selector of candidates) {
    const target = [...within.querySelectorAll<HTMLElement>(selector)].find(
      (element) => isVisible(element) && !element.closest("[inert]") && !(element as HTMLButtonElement).disabled
    );
    if (!target) continue;
    target.focus({ preventScroll: true });
    const field = target as HTMLInputElement;
    if (descriptor.selectionStart !== null && typeof field.setSelectionRange === "function") {
      try {
        field.setSelectionRange(descriptor.selectionStart, descriptor.selectionEnd ?? descriptor.selectionStart);
      } catch {
        // Not every input type supports a caret.
      }
    }
    return document.activeElement === target;
  }
  return false;
}

function focusables(container: ParentNode) {
  return [...container.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (element) => isVisible(element) && !element.closest("[inert]")
  );
}

// First field of a dialog, or its first control that is not a close button.
function focusDialogStart(dialog: HTMLElement) {
  const preferred =
    dialog.querySelector<HTMLElement>("[data-autofocus]") ??
    focusables(dialog).find((element) => element.matches("input, select, textarea")) ??
    focusables(dialog).find((element) => !element.matches(DIALOG_CLOSE)) ??
    focusables(dialog)[0];
  if (preferred) {
    preferred.focus({ preventScroll: true });
    return;
  }
  dialog.setAttribute("tabindex", "-1");
  dialog.focus({ preventScroll: true });
}

function focusViewHeading() {
  const heading = appElement.querySelector<HTMLElement>("h1");
  if (!heading) return;
  heading.setAttribute("tabindex", "-1");
  heading.focus({ preventScroll: true });
}

function topDialog(): HTMLElement | null {
  const dialogs = appElement.querySelectorAll<HTMLElement>('[role="dialog"][aria-modal="true"]');
  return dialogs.length ? dialogs[dialogs.length - 1] : null;
}

// Everything outside the open dialog becomes inert: not focusable, not
// clickable, hidden from screen readers.
function updateInert(dialog: HTMLElement | null) {
  appElement.querySelectorAll("[data-a11y-inert]").forEach((element) => {
    element.removeAttribute("inert");
    element.removeAttribute("data-a11y-inert");
  });
  if (!dialog) return;
  let node: HTMLElement = dialog;
  while (node !== appElement && node.parentElement) {
    for (const sibling of node.parentElement.children) {
      if (sibling !== node && !sibling.hasAttribute("inert")) {
        sibling.setAttribute("inert", "");
        sibling.setAttribute("data-a11y-inert", "");
      }
    }
    node = node.parentElement;
  }
}

// ----- live regions -------------------------------------------------------

function liveRegion(id: string, politeness: "polite" | "assertive") {
  let region = document.getElementById(id);
  if (!region) {
    region = document.createElement("div");
    region.id = id;
    region.className = "visually-hidden";
    region.setAttribute("aria-live", politeness);
    region.setAttribute("aria-atomic", "true");
    if (politeness === "assertive") region.setAttribute("role", "alert");
    else region.setAttribute("role", "status");
    document.body.appendChild(region);
  }
  return region;
}

export function announce(text: string, politeness: "polite" | "assertive" = "polite") {
  const region = liveRegion(politeness === "assertive" ? "a11y-live-assertive" : "a11y-live-polite", politeness);
  // Clearing first makes screen readers repeat an identical message.
  region.textContent = "";
  window.setTimeout(() => {
    region.textContent = text;
  }, 60);
}

// Every element marked with data-announce is spoken once when it appears.
function announceNewMessages() {
  const current = new Set<string>();
  appElement.querySelectorAll<HTMLElement>("[data-announce]").forEach((element) => {
    const text = element.textContent?.replace(/\s+/g, " ").trim() ?? "";
    if (!text) return;
    const politeness = element.dataset.announce === "assertive" ? "assertive" : "polite";
    const key = `${politeness}:${text}`;
    current.add(key);
    if (!announcedTexts.has(key)) announce(text, politeness);
  });
  announcedTexts = current;
}

// ----- rendering ------------------------------------------------------------

export function replaceAppHtml(html: string) {
  const previousFocus = describeFocus();
  appElement.innerHTML = html;
  afterRender(previousFocus);
}

// Call after any render that replaced part of the DOM (the full render above,
// or targeted re-renders of a list) so focus and announcements stay in sync.
export function afterRender(previousFocus: FocusDescriptor | null) {
  const dialog = topDialog();
  const dialogKey = dialog ? dialog.dataset.dialog ?? dialog.getAttribute("aria-labelledby") ?? "dialog" : null;
  updateInert(dialog);

  if (dialog && dialogKey !== lastDialogKey) {
    // A dialog opened (or another one replaced it): remember where focus came
    // from the first time, then move focus into the dialog.
    if (!lastDialogKey) focusBeforeDialog = previousFocus;
    if (!restoreFocus(previousFocus, dialog)) focusDialogStart(dialog);
  } else if (!dialog && lastDialogKey) {
    // The dialog closed: hand focus back to the control that opened it.
    if (!restoreFocus(focusBeforeDialog) && !restoreFocus(previousFocus)) focusViewHeading();
    focusBeforeDialog = null;
  } else if (!appElement.contains(document.activeElement) || document.activeElement === document.body) {
    // Same screen, but the re-render dropped focus: put it back.
    const restored = restoreFocus(previousFocus, dialog ?? appElement);
    if (!restored && dialog) focusDialogStart(dialog);
    else if (!restored && previousFocus && !firstRender) focusViewHeading();
  }

  lastDialogKey = dialogKey;
  firstRender = false;
  announceNewMessages();
}

export function captureFocus() {
  return describeFocus();
}

// ----- keyboard -------------------------------------------------------------

function onKeydown(event: KeyboardEvent) {
  if (event.defaultPrevented) return;

  if (event.key === "Escape") {
    // Inline popovers close first, then the top-most dialog.
    const popoverToggle = appElement.querySelector<HTMLButtonElement>(POPOVER_TOGGLES);
    if (popoverToggle) {
      event.preventDefault();
      // The click re-renders, so look the toggle up again before focusing it.
      const toggleSelector = selectorFor(popoverToggle, IDENTITY_ATTRIBUTES.filter((name) => popoverToggle.hasAttribute(name)));
      popoverToggle.click();
      appElement.querySelector<HTMLElement>(toggleSelector)?.focus();
      return;
    }
    const dialog = topDialog();
    const close = dialog?.querySelector<HTMLButtonElement>(DIALOG_CLOSE);
    if (close && !close.disabled) {
      event.preventDefault();
      close.click();
    }
    return;
  }

  if (event.key === "Tab") {
    const dialog = topDialog();
    if (!dialog) return;
    const items = focusables(dialog);
    if (!items.length) {
      event.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement as HTMLElement | null;
    const outside = !active || !dialog.contains(active);
    if (event.shiftKey && (active === first || outside)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || outside)) {
      event.preventDefault();
      first.focus();
    }
  }
}

document.addEventListener("keydown", onKeydown);
liveRegion("a11y-live-polite", "polite");
liveRegion("a11y-live-assertive", "assertive");
