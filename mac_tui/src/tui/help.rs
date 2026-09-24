//! In-app tutorial, adapted from the desktop app's tutorial for the terminal.

pub(crate) struct HelpTopic {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) blurb: &'static str,
    pub(crate) steps: &'static [(&'static str, &'static str)],
}

pub(crate) const TOPICS: &[HelpTopic] = &[
    HelpTopic {
        id: "aliases",
        label: "Aliases",
        blurb: "Turn long commands and paths into short words you type in the terminal.",
        steps: &[
            (
                "What an alias is",
                "An alias is a short name for something you would otherwise type in full - a folder to jump to, a file to open, or a command to run.\n\nEasyAlias stores your aliases as structured data and wires them into your shell, so typing `myproj` in a terminal can mean `cd \"$HOME/Projects/my-project\"`.\n\nIt never rewrites your whole shell config: it manages its own file and adds one source line.",
            ),
            (
                "Create one",
                "Press `n` in the Aliases tab.\n\n- Command Name - the word you'll type in the terminal.\n- Action - how the command is built: Go to Folder, Open, Run, Gradle/Maven Build, or Custom Command for anything else. Change it with ← →.\n- Path - press Tab to complete folders and files. For Custom Command this field becomes the command itself.\n\nThe Preview shows the exact shell line that will be generated. Enter on the last field (or Ctrl+S) saves.",
            ),
            (
                "Your alias list",
                "Each row is one alias. Move with ↑ ↓ (or j k).\n\n- Space or `*` - favorite; favorites are pinned to the top.\n- Enter or `e` - edit.\n- `d` - move to Trash (recoverable for 30 days).\n- `/` - search by name or generated command.\n- `f` / `F` - filter: Favorites, Git, Docker, Navigation, Build.\n- Esc - clear search and filter.",
            ),
            (
                "Suggestions",
                "Press `s` for a catalog of ready-made aliases (git, docker, build tools, navigation ...). Enter adds the selected one instantly.\n\nDon't want them? Turn them off in Settings (3) → Alias suggestions.",
            ),
            (
                "Import & backup",
                "- `i` scans your shell startup files for simple aliases you already have and imports the ones you pick. A backup of each changed file is written first.\n- `b` exports a portable JSON backup, `B` imports one - useful to move aliases between machines.\n- `t` opens the 30-day Trash: `r` restores, `d` deletes forever, `E` empties it.",
            ),
            (
                "Using them in the terminal",
                "EasyAlias writes ~/.easyalias/aliases.zsh and adds `source ~/.easyalias/aliases.zsh` to ~/.zshrc, ~/.bash_profile and ~/.bashrc once.\n\nA new terminal window has your aliases right away. In a terminal that's already open, run `source ~/.zshrc` to pick up changes.\n\nThe data is shared with the EasyAlias desktop app, so you can use both side by side.",
            ),
        ],
    },
    HelpTopic {
        id: "automations",
        label: "Automations",
        blurb: "Chain commands into one-key workflows, then run them on a schedule.",
        steps: &[
            (
                "What an automation is",
                "An automation is a named, multi-step workflow you run with one key - or automatically on a schedule.\n\nOpen the Automations tab with `2`.",
            ),
            (
                "Build one",
                "Press `n`. Give it a name and a working directory (Tab completes paths), optionally a group, then add steps:\n\n- Ctrl+N - add a Command step. Ctrl+T switches whether the next step waits for it to finish or starts as soon as it launched (background - handy for dev servers).\n- Ctrl+P - add a Wait (pause) step, 1 second to 24 hours.\n- Shift+↑ ↓ moves a step, Ctrl+X removes it.\n\nIf the working directory points at a file, the run uses the folder that contains it. Ctrl+S saves.",
            ),
            (
                "One shell session",
                "Every command step in a run shares one shell session started in the working directory.\n\nSo a `cd` or an exported variable in one step is still in effect for every step after it - exactly like a real terminal.",
            ),
            (
                "Run it",
                "Press Enter (or `r`) on an automation. The run view shows each step's status and captured output.\n\n- `s` stops the run immediately (a background process that already launched keeps going on its own).\n- ↑ ↓ picks a step to read its output, PgUp/PgDn scrolls, `f` follows the current step again.\n- Esc hides the view - the run keeps going. `o` brings it back.\n\nIf a foreground step exits non-zero, the run stops and the rest are skipped.",
            ),
            (
                "Organize",
                "Automations support the same tools as aliases: Space favorites, `g` sets a free-text group, `/` searches and `f` filters - including a Group view and one filter per group.\n\n`b` / `B` export and import automation backups, `t` opens the automation Trash.",
            ),
            (
                "Schedule it",
                "Press `c` on an automation to schedule it. Pick a trigger with ← →:\n\n- Time - a fixed HH:MM.\n- Sunrise / Sunset - that day's real event for the region you choose, recomputed daily.\n\nOptionally limit it to certain weekdays (← → and Space, or 1-7). EasyAlias hands the schedule to macOS launchd, so it fires even when EasyAlias is closed. The list shows the last run's result. Ctrl+D removes a schedule.",
            ),
        ],
    },
    HelpTopic {
        id: "keys",
        label: "All keys",
        blurb: "Everything on one page.",
        steps: &[(
            "Keyboard reference",
            "Everywhere: `1` `2` `3` or Tab switch tabs · `?` help · `q` quit · Ctrl+C quit now\n\nAliases: `n` new · Enter/`e` edit · Space favorite · `d` trash · `/` search · `f` filter · `s` suggestions · `i` import from shell · `b`/`B` backup export/import · `t` trash\n\nAutomations: `n` new · `e` edit · Enter/`r` run · `o` last run · Space favorite · `g` group · `c` schedule · `d` trash · `/` search · `f` filter · `b`/`B` backup · `t` trash\n\nForms: ↑ ↓ fields · Tab complete path · Enter next / save · Ctrl+S save · Esc cancel\n\nText fields: Ctrl+A/E start/end · Ctrl+U/K delete to start/end · Ctrl+W delete word",
        )],
    },
    HelpTopic {
        id: "support",
        label: "Support",
        blurb: "EasyAlias is free and open source. Here's how you can help.",
        steps: &[
            (
                "Why it helps",
                "Hi, I'm Hannes - I build EasyAlias alongside my Software Engineering studies, and it's free and open source.\n\nSponsorship is what lets me keep fixing bugs, shipping features, and keeping it free for everyone.\n\nPress `s` to open GitHub Sponsors.",
            ),
            (
                "Other ways",
                "- Star the repository on GitHub\n- Share feedback in the r/easyalias subreddit\n- Visit easyalias.org\n\nThe links are also in Settings (3). Every bit genuinely helps. Thank you!",
            ),
        ],
    },
];
