// In-app tutorial content and modal.

import { redditUrl, repoUrl, sponsorUrl, websiteUrl } from "./constants";
import { escapeHtml } from "./html";
import { clearMessages } from "./messages";
import { openExternalLink } from "./platform";
import { render } from "./render";
import { state } from "./state";
import tutorialAliasListImg from "./tutorial/alias-list.png";
import tutorialCardImg from "./tutorial/automation-card.png";
import tutorialEditorImg from "./tutorial/automation-editor.png";
import tutorialCreateImg from "./tutorial/create.png";
import tutorialScheduleImg from "./tutorial/schedule.png";
import tutorialShortcutImg from "./tutorial/shortcut.png";
import tutorialSuggestionsImg from "./tutorial/suggestions.png";
import type { TutorialContent, TutorialTopic } from "./types";

export const TUTORIALS: Record<TutorialTopic, TutorialContent> = {
  aliases: {
    label: "The alias area",
    icon: "square-terminal",
    blurb: "Turn long commands and paths into short words you type in the terminal.",
    steps: [
      {
        title: "What an alias is",
        body: `
          <p>An <strong>alias</strong> is a short name for something you would otherwise type in full &mdash; a folder to jump to, a file to open, or a command to run.</p>
          <p>EasyAlias stores your aliases as structured data and wires them into your shell, so typing <code>myproj</code> in a terminal can mean <code>cd "$HOME/Projects/my-project"</code>.</p>
          <p>It never rewrites your whole shell config: it manages its own file and adds one <code>source</code> line.</p>`
      },
      {
        title: "Create one",
        body: `
          <p>Use the <strong>Create Alias</strong> panel on the left:</p>
          <ul>
            <li><strong>Command Name</strong> &mdash; the word you'll type in the terminal.</li>
            <li><strong>Location / File / Command</strong> &mdash; a path, or free text for a custom command. The <strong>File</strong> and <strong>Folder</strong> buttons open the native picker.</li>
            <li><strong>Action</strong> &mdash; how the command is built: <em>Go to Folder</em>, <em>Open</em>, <em>Execute</em>, <em>Gradle/Maven Build</em>, or <em>Custom Command</em> for anything else.</li>
          </ul>
          <p>The <strong>Preview</strong> shows the exact shell line that will be generated. Click <strong>Add</strong> to save it.</p>`,
        image: tutorialCreateImg
      },
      {
        title: "Your alias list",
        body: `
          <p>Each row on the right is one alias. From a row you can:</p>
          <ul>
            <li>Click the <strong>star</strong> to make it a favorite &mdash; favorites are pinned to the top.</li>
            <li>Click <strong>Edit</strong> to change any field.</li>
            <li>Click <strong>&times;</strong> to move it to <strong>Trash</strong> (recoverable for 30 days).</li>
          </ul>
          <p>The <strong>search</strong> box matches a name or its generated command, and the <strong>filter</strong> dropdown narrows to Favorites, Git, Docker, Navigation, or Build.</p>`,
        image: tutorialAliasListImg
      },
      {
        title: "Suggestions",
        body: `
          <p>The <strong>Suggestions</strong> bar holds a catalog of ready-made aliases (git, docker, build tools, navigation&hellip;). Open it and click <strong>Use</strong> to add one instantly &mdash; no second step.</p>
          <p>Don't want it? Turn it off under the gear icon &rarr; <strong>Alias suggestions</strong>.</p>`,
        image: tutorialSuggestionsImg
      },
      {
        title: "Import & backup",
        body: `
          <p>In the header:</p>
          <ul>
            <li>The <strong>terminal</strong> icon scans your shell startup file for simple aliases you already have and lets you import the ones you pick (a backup is written first).</li>
            <li>The <strong>up</strong> / <strong>down</strong> file icons export and import a portable JSON backup &mdash; useful to move aliases between machines.</li>
            <li>The <strong>trash</strong> icon opens the 30-day recovery bin.</li>
          </ul>`
      },
      {
        title: "Using them in the terminal",
        body: `
          <p>EasyAlias writes <code>~/.easyalias/aliases.sh</code> and adds <code>source ~/.easyalias/aliases.sh</code> to your shell startup file (<code>~/.bashrc</code> or <code>~/.zshrc</code>) once.</p>
          <p>A <strong>new</strong> terminal has your aliases right away. In one that's already open, run <code>source ~/.bashrc</code> (or <code>~/.zshrc</code>) to pick up changes.</p>`
      }
    ]
  },
  automations: {
    label: "The automations area",
    icon: "workflow",
    blurb: "Chain commands into one-click workflows, then run them on a schedule or a shortcut.",
    steps: [
      {
        title: "What an automation is",
        body: `
          <p>An <strong>automation</strong> is a named, multi-step workflow you run with one click &mdash; or automatically on a schedule or a keyboard shortcut.</p>
          <p>Open the area with the <strong>&#9658; play</strong> icon at the top of the header. It's a separate workspace from your aliases.</p>`
      },
      {
        title: "Build one",
        body: `
          <p><strong>Create automation</strong> asks for a name and a <strong>working directory</strong>, then you add steps:</p>
          <ul>
            <li><strong>Command</strong> &mdash; a shell command. <em>Continue when</em> decides whether the next step waits for it to finish, or starts as soon as the process launches (handy for dev servers).</li>
            <li><strong>Wait</strong> &mdash; a pause from 1 second to 24 hours.</li>
          </ul>
          <p>If the working-directory field points at a file, the run uses the folder that contains it.</p>`,
        image: tutorialEditorImg
      },
      {
        title: "One shell session",
        body: `
          <p>Every command step in a run shares <strong>one shell session</strong> started in the working directory. So a <code>cd</code> or an exported variable in one step is still in effect for every step after it &mdash; exactly like a real terminal.</p>`
      },
      {
        title: "Run it",
        body: `
          <p>Click <strong>Run</strong> on a card. A progress dialog shows each step's status and captured output. <strong>Stop</strong> ends the run immediately (a background process that already launched keeps going on its own). If a foreground step exits non-zero, the run stops and the rest are marked skipped.</p>`
      },
      {
        title: "Organize",
        body: `
          <p>Cards support the same tools as aliases: a <strong>favorite</strong> star, a free-text <strong>group</strong> label, and search + filter (including a <strong>Group view</strong>). The header has its own <strong>backup</strong> and <strong>trash</strong> icons for automations.</p>`,
        image: tutorialCardImg
      },
      {
        title: "Schedule it (Timed Automation)",
        body: `
          <p>The <strong>clock</strong> icon on a card opens a schedule. Pick a trigger:</p>
          <ul>
            <li><strong>Time</strong> &mdash; a fixed <code>HH:MM</code>.</li>
            <li><strong>Sunrise</strong> / <strong>Sunset</strong> &mdash; that day's real event for a <strong>region</strong> you choose from a dropdown, recomputed daily.</li>
          </ul>
          <p>Optionally limit it to certain weekdays. EasyAlias hands the schedule to systemd user timers, so it fires <strong>even when EasyAlias is closed</strong>. The card shows the last run's result.</p>`,
        image: tutorialScheduleImg
      },
      {
        title: "Keyboard shortcut",
        body: `
          <p>The <strong>keyboard</strong> icon on a card records a global shortcut (for example <kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>L</kbd>). While EasyAlias runs, pressing it anywhere fires that automation.</p>
          <p>Under the gear icon, <strong>Automation shortcuts</strong> chooses what a press does: bring the run window forward, or run silently in the background.</p>
          <p>Closing the window keeps EasyAlias in the <strong>system tray</strong> so shortcuts and schedules keep working; quit it from the tray icon.</p>`,
        image: tutorialShortcutImg
      }
    ]
  },
  support: {
    label: "How to support me",
    icon: "heart",
    blurb: "EasyAlias is free and open source. Here's how you can help.",
    steps: [
      {
        title: "Why it helps",
        body: `
          <p>Hi, I'm <strong>Hannes</strong> &mdash; I build EasyAlias alongside my Software Engineering studies, and it's free and open source.</p>
          <p>Sponsorship is what lets me keep fixing bugs, shipping features, and keeping it free for everyone.</p>
          <p><a href="${sponsorUrl}" target="_blank" rel="noreferrer" data-external-link>Become a GitHub Sponsor &rarr;</a></p>`
      },
      {
        title: "Other ways",
        body: `
          <ul>
            <li>&#11088; <a href="${repoUrl}" target="_blank" rel="noreferrer" data-external-link>Star the repository on GitHub</a></li>
            <li>&#128172; <a href="${redditUrl}" target="_blank" rel="noreferrer" data-external-link>Share feedback in the EasyAlias subreddit</a></li>
            <li>&#127760; <a href="${websiteUrl}" target="_blank" rel="noreferrer" data-external-link>Visit the website</a></li>
            <li>&#11088; Leave a review on the Mac App Store if you use that edition</li>
          </ul>
          <p>Every bit genuinely helps. Thank you!</p>`
      }
    ]
  }
};

export const TUTORIAL_ORDER: TutorialTopic[] = ["aliases", "automations", "support"];

export function openTutorial() {
  clearMessages();
  state.tutorialOpen = true;
  state.tutorialTopic = null;
  state.tutorialStep = 0;
  render();
}

export function closeTutorial() {
  state.tutorialOpen = false;
  state.tutorialTopic = null;
  state.tutorialStep = 0;
  render();
}

export function startTutorial(topic: TutorialTopic) {
  state.tutorialTopic = topic;
  state.tutorialStep = 0;
  render();
}

export function tutorialGoTo(step: number) {
  if (!state.tutorialTopic) return;
  const total = TUTORIALS[state.tutorialTopic].steps.length;
  if (step < 0) {
    // Back from the first slide returns to the picker.
    state.tutorialTopic = null;
    state.tutorialStep = 0;
  } else if (step >= total) {
    closeTutorial();
    return;
  } else {
    state.tutorialStep = step;
  }
  render();
}

export function renderTutorialModal(): string {
  if (!state.tutorialOpen) return "";

  if (!state.tutorialTopic) {
    const cards = TUTORIAL_ORDER.map((topic, index) => {
      const t = TUTORIALS[topic];
      return `
        <button type="button" class="tutorial-choice" data-tutorial-action="start" data-topic="${topic}">
          <span class="tutorial-choice-index">${index + 1}</span>
          <span class="tutorial-choice-icon"><i data-lucide="${t.icon}"></i></span>
          <span class="tutorial-choice-text">
            <strong>${escapeHtml(t.label)}</strong>
            <span>${escapeHtml(t.blurb)}</span>
          </span>
        </button>`;
    }).join("");

    return `
      <section class="modal-layer" role="presentation">
        <section class="modal-card tutorial-card" role="dialog" aria-modal="true" aria-labelledby="tutorial-title">
          <div class="modal-title">
            <div>
              <p class="eyebrow">Tutorial</p>
              <h2 id="tutorial-title">What would you like to learn?</h2>
            </div>
            <button class="ghost-button modal-close" type="button" data-tutorial-action="close">Close</button>
          </div>
          <div class="tutorial-choices">${cards}</div>
        </section>
      </section>`;
  }

  const tutorial = TUTORIALS[state.tutorialTopic];
  const total = tutorial.steps.length;
  const step = tutorial.steps[Math.min(state.tutorialStep, total - 1)];
  const dots = tutorial.steps
    .map(
      (_, index) =>
        `<span class="tutorial-dot ${index === state.tutorialStep ? "is-current" : ""} ${index < state.tutorialStep ? "is-done" : ""}"></span>`
    )
    .join("");
  const isLast = state.tutorialStep >= total - 1;

  return `
    <section class="modal-layer" role="presentation">
      <section class="modal-card tutorial-card" role="dialog" aria-modal="true" aria-labelledby="tutorial-step-title">
        <div class="modal-title">
          <div>
            <p class="eyebrow">${escapeHtml(tutorial.label)} &middot; ${state.tutorialStep + 1} / ${total}</p>
            <h2 id="tutorial-step-title">${escapeHtml(step.title)}</h2>
          </div>
          <button class="ghost-button modal-close" type="button" data-tutorial-action="close">Close</button>
        </div>
        <div class="tutorial-body">
          ${step.image ? `<img class="tutorial-image" src="${step.image}" alt="${escapeHtml(step.title)}" loading="lazy" />` : ""}
          ${step.body}
        </div>
        <div class="tutorial-footer">
          <div class="tutorial-dots" aria-hidden="true">${dots}</div>
          <div class="tutorial-nav">
            <button type="button" class="ghost-button" data-tutorial-action="prev">${state.tutorialStep === 0 ? "All topics" : "Back"}</button>
            <button type="button" class="primary-button" data-tutorial-action="next">${isLast ? "Done" : "Next"}</button>
          </div>
        </div>
      </section>
    </section>`;
}

// Bound from both bindEvents() and bindAutomationEvents() so the tutorial
// overlay works over the alias view and the automations view alike.
export function bindTutorialEvents() {
  document.querySelectorAll<HTMLButtonElement>("[data-tutorial-action]").forEach((button) => {
    button.addEventListener("click", () => {
      const action = button.dataset.tutorialAction;
      if (action === "close") closeTutorial();
      else if (action === "start") startTutorial(button.dataset.topic as TutorialTopic);
      else if (action === "prev") tutorialGoTo(state.tutorialStep - 1);
      else if (action === "next") tutorialGoTo(state.tutorialStep + 1);
    });
  });
  document
    .querySelectorAll<HTMLAnchorElement>(".tutorial-body [data-external-link], .tutorial-choices [data-external-link]")
    .forEach((link) => link.addEventListener("click", openExternalLink));
}
