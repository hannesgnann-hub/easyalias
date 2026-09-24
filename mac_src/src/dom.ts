// The root element the whole UI renders into.

// Vite mounts the app into <main id="app"> from index.html.
export const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
  throw new Error("App container not found");
}

export const appElement = app;
