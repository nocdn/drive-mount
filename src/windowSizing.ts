import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

const WINDOW_WIDTH = 560;

function measureRequiredInnerHeight(appEl: HTMLElement): number {
  const lastChild = appEl.lastElementChild;
  if (!(lastChild instanceof HTMLElement)) {
    return Math.ceil(appEl.getBoundingClientRect().height);
  }

  const appTop = appEl.getBoundingClientRect().top;
  const lastBottom = lastChild.getBoundingClientRect().bottom;
  const paddingBottom = Number.parseFloat(getComputedStyle(appEl).paddingBottom) || 0;
  return Math.ceil(lastBottom - appTop + paddingBottom);
}

async function fitWindowToContent() {
  const appEl = document.getElementById("app");
  if (!appEl) {
    return;
  }

  const tauriWindow = getCurrentWebviewWindow();
  let targetHeight = measureRequiredInnerHeight(appEl);

  await tauriWindow.setSize(new LogicalSize(WINDOW_WIDTH, targetHeight));

  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => resolve());
  });

  const clippedBy = measureRequiredInnerHeight(appEl) - window.innerHeight;
  if (clippedBy > 0) {
    targetHeight += Math.ceil(clippedBy);
    await tauriWindow.setSize(new LogicalSize(WINDOW_WIDTH, targetHeight));
  }
}

export function scheduleFitWindow() {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void fitWindowToContent();
    });
  });
}

export function startWindowFitWatcher() {
  const appEl = document.getElementById("app");
  if (!appEl) {
    return;
  }

  const observer = new ResizeObserver(() => {
    scheduleFitWindow();
  });
  observer.observe(appEl);
  scheduleFitWindow();
}

export function installVisibilityWindowFit() {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
      scheduleFitWindow();
    }
  });
}
