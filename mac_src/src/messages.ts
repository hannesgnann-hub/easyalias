// Notice/error banner handling.

import { render } from "./render";
import { state } from "./state";

// Message helpers keep the visible notice/error state separate from form data.
export function cancelMessageDismissal() {
  if (state.messageDismissTimer !== null) {
    clearTimeout(state.messageDismissTimer);
    state.messageDismissTimer = null;
  }

  state.scheduledMessageKey = "";
}

export function clearMessages() {
  cancelMessageDismissal();
  state.notice = "";
  state.error = "";
}

export function clearRenderedMessages() {
  document.querySelector(".notice")?.remove();
  document.querySelector(".error")?.remove();
}

export function dismissMessage() {
  clearMessages();
  render();
}

// Every new global status message gets a fresh three-second lifetime. Re-renders
// with the same message keep the existing deadline instead of extending it.
export function scheduleMessageDismissal() {
  const messageKey = state.error ? `error:${state.error}` : state.notice ? `notice:${state.notice}` : "";

  // "Keep messages" (Settings > Accessibility): nothing disappears on its own,
  // so there is time to read it or to have it read aloud.
  if (!messageKey || state.appSettings.keepMessages) {
    cancelMessageDismissal();
    return;
  }

  if (state.messageDismissTimer !== null && state.scheduledMessageKey === messageKey) return;

  cancelMessageDismissal();
  state.scheduledMessageKey = messageKey;
  state.messageDismissTimer = setTimeout(() => {
    state.messageDismissTimer = null;
    state.scheduledMessageKey = "";
    state.notice = "";
    state.error = "";
    render();
  }, 3000);
}
