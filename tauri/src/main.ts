import { invoke } from "@tauri-apps/api/core";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { isEnabled, enable, disable } from "@tauri-apps/plugin-autostart";

type CloudProvider = "B2" | "GoogleDrive" | "Seedbox";

interface BucketMount {
  bucketName: string;
  mountPath: string;
  driveLetter: string;
}

interface AppSettings {
  selectedProvider: CloudProvider;
  buckets: BucketMount[];
  startAtLogin: boolean;
  startMinimized: boolean;
}

interface LoadedSettings {
  settings: AppSettings;
  hasSavedCredentials: boolean;
  applicationKeyId: string;
  applicationKey: string;
}

interface LogLine {
  level: string;
  message: string;
  timestamp: string;
}

let platform = "macos";
let mounted = false;
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

  // setSize sets the inner (client) area — CSS pixels, not outer frame size.
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

function scheduleFitWindow() {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void fitWindowToContent();
    });
  });
}

function startWindowFitWatcher() {
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

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    scheduleFitWindow();
  }
});

const providerSelect = document.getElementById("provider") as HTMLSelectElement;
const b2Panel = document.getElementById("b2-panel") as HTMLDivElement;
const stubPanel = document.getElementById("stub-panel") as HTMLDivElement;
const keyIdInput = document.getElementById("key-id") as HTMLInputElement;
const keyInput = document.getElementById("key") as HTMLInputElement;
const bucketsList = document.getElementById("buckets-list") as HTMLDivElement;
const bucketsHelp = document.getElementById("buckets-help") as HTMLParagraphElement;
const addBucketBtn = document.getElementById("add-bucket") as HTMLButtonElement;
const startAtLoginCheckbox = document.getElementById("start-at-login") as HTMLInputElement;
const startMinimizedCheckbox = document.getElementById("start-minimized") as HTMLInputElement;
const startMinimizedLabel = document.getElementById("start-minimized-label") as HTMLSpanElement;
const btnMount = document.getElementById("btn-mount") as HTMLButtonElement;
const btnUnmount = document.getElementById("btn-unmount") as HTMLButtonElement;
const btnOpenLogs = document.getElementById("btn-open-logs") as HTMLButtonElement;
const btnClearLogs = document.getElementById("btn-clear-logs") as HTMLButtonElement;
const btnCopyLogs = document.getElementById("btn-copy-logs") as HTMLButtonElement;
const btnRestart = document.getElementById("btn-restart") as HTMLButtonElement;
const logsEl = document.getElementById("logs") as HTMLPreElement;

function isSelectableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Node)) {
    return false;
  }

  let el: Element | null = target instanceof Element ? target : target.parentElement;
  while (el) {
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
      return true;
    }
    if (el.classList.contains("logs")) {
      return true;
    }
    el = el.parentElement;
  }

  return false;
}

function preventUiTextSelection() {
  document.addEventListener(
    "selectstart",
    (event) => {
      if (!isSelectableTarget(event.target)) {
        event.preventDefault();
      }
    },
    true,
  );
}

preventUiTextSelection();

function appendLog(line: LogLine) {
  logsEl.textContent += `[${line.timestamp}] [${line.level}] ${line.message}\n`;
  logsEl.scrollTop = logsEl.scrollHeight;
}

function updateMountButtons() {
  btnMount.disabled = mounted;
  btnUnmount.disabled = !mounted;
}

function updateProviderPanels() {
  const provider = providerSelect.value as CloudProvider;
  if (provider === "B2") {
    b2Panel.classList.remove("hidden");
    stubPanel.classList.add("hidden");
  } else {
    b2Panel.classList.add("hidden");
    stubPanel.classList.remove("hidden");
  }
  scheduleFitWindow();
}

function defaultMountPath(bucketName: string): string {
  return `~/Drives/${bucketName}`;
}

function createBucketRow(bucket: BucketMount = { bucketName: "", mountPath: "", driveLetter: "Z" }) {
  const row = document.createElement("div");
  row.className = "bucket-row";

  const nameLabel = document.createElement("label");
  nameLabel.textContent = "Bucket";
  const nameInput = document.createElement("input");
  nameInput.type = "text";
  nameInput.className = "bucket-name-input";
  nameInput.value = bucket.bucketName;

  const bucketGroup = document.createElement("div");
  bucketGroup.className = "bucket-inline-group";
  bucketGroup.append(nameLabel, nameInput);
  row.appendChild(bucketGroup);

  let pathInput: HTMLInputElement | null = null;
  let driveInput: HTMLInputElement | null = null;

  if (platform === "macos") {
    const pathLabel = document.createElement("label");
    pathLabel.textContent = "Mount";
    pathInput = document.createElement("input");
    pathInput.type = "text";
    pathInput.className = "bucket-mount-input";
    pathInput.value = bucket.mountPath || (bucket.bucketName ? defaultMountPath(bucket.bucketName) : "");

    const browseBtn = document.createElement("button");
    browseBtn.type = "button";
    browseBtn.textContent = "Browse";
    browseBtn.addEventListener("click", async () => {
      const selected = await invoke<string | null>("browse_folder");
      if (selected && pathInput) {
        pathInput.value = selected;
      }
    });

    const mountGroup = document.createElement("div");
    mountGroup.className = "bucket-inline-group bucket-mount-group";
    mountGroup.append(pathLabel, pathInput, browseBtn);

    nameInput.addEventListener("input", () => {
      if (pathInput && !pathInput.dataset.manual) {
        pathInput.value = nameInput.value ? defaultMountPath(nameInput.value) : "";
      }
    });
    pathInput.addEventListener("input", () => {
      pathInput!.dataset.manual = "true";
    });

    row.appendChild(mountGroup);
  } else {
    const driveLabel = document.createElement("label");
    driveLabel.textContent = "Drive";
    driveInput = document.createElement("input");
    driveInput.type = "text";
    driveInput.className = "bucket-drive-input";
    driveInput.maxLength = 3;
    driveInput.value = bucket.driveLetter || "Z";

    const driveGroup = document.createElement("div");
    driveGroup.className = "bucket-inline-group";
    driveGroup.append(driveLabel, driveInput);
    row.appendChild(driveGroup);
  }

  const removeBtn = document.createElement("button");
  removeBtn.type = "button";
  removeBtn.textContent = "Remove";
  removeBtn.addEventListener("click", () => {
    if (bucketsList.children.length <= 1) {
      nameInput.value = "";
      if (pathInput) pathInput.value = "";
      if (driveInput) driveInput.value = "Z";
      return;
    }
    row.remove();
    scheduleFitWindow();
  });
  row.appendChild(removeBtn);

  bucketsList.appendChild(row);
  scheduleFitWindow();
}

function collectBuckets(): BucketMount[] {
  const buckets: BucketMount[] = [];
  bucketsList.querySelectorAll(".bucket-row").forEach((row) => {
    const inputs = row.querySelectorAll("input");
    const bucketName = inputs[0]?.value.trim() ?? "";
    const second = inputs[1]?.value.trim() ?? "";
    if (platform === "macos") {
      buckets.push({
        bucketName,
        mountPath: second,
        driveLetter: "",
      });
    } else {
      buckets.push({
        bucketName,
        mountPath: "",
        driveLetter: second,
      });
    }
  });
  return buckets;
}

function renderBuckets(buckets: BucketMount[]) {
  bucketsList.innerHTML = "";
  const rows = buckets.length > 0 ? buckets : [{ bucketName: "", mountPath: "", driveLetter: "Z" }];
  rows.forEach((bucket) => createBucketRow(bucket));
  scheduleFitWindow();
}

function collectSettings(): AppSettings {
  return {
    selectedProvider: providerSelect.value as CloudProvider,
    buckets: collectBuckets(),
    startAtLogin: startAtLoginCheckbox.checked,
    startMinimized: startMinimizedCheckbox.checked,
  };
}

async function applyAutostart(enabled: boolean) {
  try {
    const current = await isEnabled();
    if (enabled && !current) {
      await enable();
    } else if (!enabled && current) {
      await disable();
    }
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: `Start at login update failed: ${String(err)}`,
      timestamp: new Date().toLocaleTimeString(),
    });
  }
}

async function savePreferences() {
  const settings = collectSettings();
  await invoke("save_settings_cmd", { settings });
  await applyAutostart(settings.startAtLogin);
}

async function loadUi() {
  platform = await invoke<string>("get_platform");

  if (platform === "windows") {
    bucketsHelp.textContent = "Each bucket is mounted directly to its own drive letter.";
    startMinimizedLabel.textContent = "Start minimized to tray";
  }

  const loaded = await invoke<LoadedSettings>("load_settings_cmd");
  providerSelect.value = loaded.settings.selectedProvider;
  keyIdInput.value = loaded.applicationKeyId;
  keyInput.value = loaded.applicationKey;
  startAtLoginCheckbox.checked = loaded.settings.startAtLogin;
  startMinimizedCheckbox.checked = loaded.settings.startMinimized;
  renderBuckets(loaded.settings.buckets);
  updateProviderPanels();
  await applyAutostart(loaded.settings.startAtLogin);

  mounted = await invoke<boolean>("is_mounted");
  updateMountButtons();

  const fuseOk = await invoke<boolean>("is_fuse_installed_cmd");
  if (!fuseOk) {
    appendLog({
      level: "ERROR",
      message:
        platform === "macos"
          ? "macFUSE is not installed or has not been enabled."
          : "WinFsp is not installed.",
      timestamp: new Date().toLocaleTimeString(),
    });
  }

  appendLog({
    level: "INFO",
    message: "Cloud Drive Mount started. Use the tray icon to reopen Settings after closing this window.",
    timestamp: new Date().toLocaleTimeString(),
  });

  startWindowFitWatcher();
}

providerSelect.addEventListener("change", () => {
  updateProviderPanels();
});

addBucketBtn.addEventListener("click", () => {
  createBucketRow();
});

startAtLoginCheckbox.addEventListener("change", () => {
  void savePreferences();
});

startMinimizedCheckbox.addEventListener("change", () => {
  void savePreferences();
});

btnMount.addEventListener("click", async () => {
  try {
    const settings = collectSettings();
    await invoke("save_settings_cmd", { settings });
    await applyAutostart(settings.startAtLogin);

    if (keyIdInput.value.trim() || keyInput.value.trim()) {
      await invoke("save_b2_credentials_cmd", {
        credentials: {
          applicationKeyId: keyIdInput.value.trim(),
          applicationKey: keyInput.value.trim(),
        },
      });
    }

    await invoke("mount_all", {
      request: {
        applicationKeyId: keyIdInput.value.trim(),
        applicationKey: keyInput.value.trim(),
        buckets: settings.buckets,
      },
    });
    mounted = true;
    updateMountButtons();
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: new Date().toLocaleTimeString(),
    });
  }
});

btnUnmount.addEventListener("click", async () => {
  try {
    await invoke("unmount_all");
    mounted = false;
    updateMountButtons();
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: new Date().toLocaleTimeString(),
    });
  }
});

btnOpenLogs.addEventListener("click", () => {
  void invoke("open_log_folder").catch((err) => {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: new Date().toLocaleTimeString(),
    });
  });
});

btnClearLogs.addEventListener("click", () => {
  logsEl.textContent = "";
  void invoke("clear_logs");
});

btnCopyLogs.addEventListener("click", async () => {
  const text = logsEl.textContent ?? "";
  if (!text) {
    return;
  }
  try {
    await navigator.clipboard.writeText(text);
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: `Copy failed: ${String(err)}`,
      timestamp: new Date().toLocaleTimeString(),
    });
  }
});

btnRestart.addEventListener("click", async () => {
  try {
    await savePreferences();
    await invoke("restart_app");
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: new Date().toLocaleTimeString(),
    });
  }
});

void listen<LogLine>("log-line", (event) => {
  appendLog(event.payload);
});

void listen<{ mounted: boolean }>("mount-state-changed", (event) => {
  mounted = event.payload.mounted;
  updateMountButtons();
});

void loadUi();
