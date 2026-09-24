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

export function scheduleMessageDismissal() {
  const messageKey = state.error ? `error:${state.error}` : state.notice ? `notice:${state.notice}` : "";
  if (!messageKey) {
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
