const rootEl = document.getElementById("root");

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function showFatal(message: string): void {
  if (!rootEl) return;
  rootEl.innerHTML = `<div style="padding:16px;font-family:Consolas,monospace;white-space:pre-wrap;color:#b00020;background:#fff;">
LocalTrans bootstrap failed.

${escapeHtml(message)}
</div>`;
}

window.addEventListener("error", (event) => {
  const stack = (event.error as Error | undefined)?.stack ?? "";
  showFatal(`${event.message}\n${stack}`);
});

window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason instanceof Error ? event.reason.stack ?? event.reason.message : String(event.reason);
  showFatal(`Unhandled rejection:\n${reason}`);
});

import("./main").catch((error) => {
  const message = error instanceof Error ? error.stack ?? error.message : String(error);
  showFatal(message);
});

