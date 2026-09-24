// Built-in alias suggestions.

import type { AliasSuggestion } from "../types";

// Conservative macOS defaults that are useful without modifying or deleting data.
// They are ordered in themed groups so each nine-item page remains easy to scan.
// Clicking Use turns one of these templates directly into a persisted AliasEntry.
export const aliasSuggestions: AliasSuggestion[] = [
  {
    id: "git-status",
    name: "gs",
    path: "",
    action: "custom",
    customCommand: "git status --short --branch",
    description: "Compact Git status"
  },
  {
    id: "git-add-all",
    name: "gaa",
    path: "",
    action: "custom",
    customCommand: "git add --all",
    description: "Stage all Git changes"
  },
  {
    id: "git-commit",
    name: "gc",
    path: "",
    action: "custom",
    customCommand: "git commit",
    description: "Create a Git commit"
  },
  {
    id: "git-commit-message",
    name: "gcm",
    path: "",
    action: "custom",
    customCommand: "git commit -m",
    description: "Commit with a message"
  },
  {
    id: "git-push",
    name: "gp",
    path: "",
    action: "custom",
    customCommand: "git push",
    description: "Push the current branch"
  },
  {
    id: "git-pull-rebase",
    name: "gpl",
    path: "",
    action: "custom",
    customCommand: "git pull --rebase",
    description: "Pull with rebase"
  },
  {
    id: "git-branch",
    name: "gb",
    path: "",
    action: "custom",
    customCommand: "git branch",
    description: "List local Git branches"
  },
  {
    id: "git-switch",
    name: "gsw",
    path: "",
    action: "custom",
    customCommand: "git switch",
    description: "Switch Git branches"
  },
  {
    id: "git-diff",
    name: "gd",
    path: "",
    action: "custom",
    customCommand: "git diff",
    description: "Show unstaged changes"
  },
  {
    id: "git-diff-staged",
    name: "gds",
    path: "",
    action: "custom",
    customCommand: "git diff --staged",
    description: "Show staged changes"
  },
  {
    id: "git-log-graph",
    name: "glog",
    path: "",
    action: "custom",
    customCommand: "git log --oneline --graph --decorate --all",
    description: "Compact Git history graph"
  },
  {
    id: "git-stash",
    name: "gstash",
    path: "",
    action: "custom",
    customCommand: "git stash push",
    description: "Stash current changes"
  },
  {
    id: "docker-compose-up",
    name: "dcu",
    path: "",
    action: "custom",
    customCommand: "docker compose up -d",
    description: "Start Docker Compose"
  },
  {
    id: "docker-compose-down",
    name: "dcd",
    path: "",
    action: "custom",
    customCommand: "docker compose down",
    description: "Stop Docker Compose"
  },
  {
    id: "docker-compose-logs",
    name: "dcl",
    path: "",
    action: "custom",
    customCommand: "docker compose logs -f",
    description: "Follow Compose logs"
  },
  {
    id: "docker-compose-build",
    name: "dcb",
    path: "",
    action: "custom",
    customCommand: "docker compose build",
    description: "Build Compose services"
  },
  {
    id: "docker-compose-restart",
    name: "dcr",
    path: "",
    action: "custom",
    customCommand: "docker compose restart",
    description: "Restart Compose services"
  },
  {
    id: "docker-ps",
    name: "dps",
    path: "",
    action: "custom",
    customCommand: "docker ps",
    description: "List running containers"
  },
  {
    id: "docker-images",
    name: "di",
    path: "",
    action: "custom",
    customCommand: "docker images",
    description: "List local Docker images"
  },
  {
    id: "docker-disk-usage",
    name: "ddf",
    path: "",
    action: "custom",
    customCommand: "docker system df",
    description: "Show Docker disk usage"
  },
  {
    id: "docker-exec",
    name: "dex",
    path: "",
    action: "custom",
    customCommand: "docker exec -it",
    description: "Run a command in a container"
  },
  {
    id: "gradle-wrapper",
    name: "gw",
    path: "",
    action: "custom",
    customCommand: "./gradlew",
    description: "Run the Gradle wrapper"
  },
  {
    id: "gradle-wrapper-build",
    name: "gwb",
    path: "",
    action: "custom",
    customCommand: "./gradlew build",
    description: "Build with Gradle wrapper"
  },
  {
    id: "gradle-wrapper-test",
    name: "gwtest",
    path: "",
    action: "custom",
    customCommand: "./gradlew test",
    description: "Run Gradle tests"
  },
  {
    id: "maven-wrapper",
    name: "mvnw",
    path: "",
    action: "custom",
    customCommand: "./mvnw",
    description: "Run the Maven wrapper"
  },
  {
    id: "maven-wrapper-build",
    name: "mvnb",
    path: "",
    action: "custom",
    customCommand: "./mvnw clean package",
    description: "Build with Maven wrapper"
  },
  {
    id: "maven-wrapper-test",
    name: "mvnt",
    path: "",
    action: "custom",
    customCommand: "./mvnw test",
    description: "Run Maven tests"
  },
  {
    id: "npm-install",
    name: "ni",
    path: "",
    action: "custom",
    customCommand: "npm install",
    description: "Install npm dependencies"
  },
  {
    id: "npm-run-dev",
    name: "nrd",
    path: "",
    action: "custom",
    customCommand: "npm run dev",
    description: "Start the npm dev script"
  },
  {
    id: "npm-run-build",
    name: "nrb",
    path: "",
    action: "custom",
    customCommand: "npm run build",
    description: "Run the npm build script"
  },
  {
    id: "list-details",
    name: "ll",
    path: "",
    action: "custom",
    customCommand: "ls -lah",
    description: "Detailed file list"
  },
  {
    id: "python-server",
    name: "serve",
    path: "",
    action: "custom",
    customCommand: "python3 -m http.server",
    description: "Serve the current folder"
  },
  {
    id: "list-ports",
    name: "ports",
    path: "",
    action: "custom",
    customCommand: "lsof -nP -iTCP -sTCP:LISTEN",
    description: "Show listening TCP ports"
  },
  {
    id: "downloads-folder",
    name: "downloads",
    path: "~/Downloads",
    action: "navigate",
    customCommand: "",
    description: "Jump to Downloads"
  },
  {
    id: "open-finder",
    name: "finder",
    path: "~",
    action: "open",
    customCommand: "",
    description: "Open your home folder"
  },
  {
    id: "reload-shell",
    name: "reloadshell",
    path: "",
    action: "custom",
    customCommand: 'exec "$SHELL" -l',
    description: "Reload the current login shell"
  }
];
