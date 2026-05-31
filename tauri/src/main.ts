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

interface GoogleDriveSettings {
  remoteName: string;
  remotePath: string;
  rootFolderId: string;
  mountPath: string;
  driveLetter: string;
}

interface SeedboxSettings {
  remoteName: string;
  host: string;
  username: string;
  port: number;
  remotePath: string;
  mountPath: string;
  driveLetter: string;
  allowUnverifiedCertificate: boolean;
  readOnly: boolean;
}

interface AppSettings {
  selectedProvider: CloudProvider;
  buckets: BucketMount[];
  googleDrive: GoogleDriveSettings;
  seedbox: SeedboxSettings;
  startAtLogin: boolean;
  startMinimized: boolean;
}

interface LoadedSettings {
  settings: AppSettings;
  hasSavedCredentials: boolean;
  applicationKeyId: string;
  applicationKey: string;
  isGoogleDriveConfigured: boolean;
  isSeedboxConfigured: boolean;
  hasSavedSeedboxPassword: boolean;
}

interface LogLine {
  level: string;
  message: string;
  timestamp: string;
}

let platform = "macos";
let mounted = false;
let googleDriveConnected = false;
let seedboxConfigured = false;
const WINDOW_WIDTH = 560;
const DEFAULT_SEEDBOX_MOUNT_PATH = "~/Drives/Seedbox";

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
const gdrivePanel = document.getElementById("gdrive-panel") as HTMLDivElement;
const seedboxPanel = document.getElementById("seedbox-panel") as HTMLDivElement;
const seedboxHostInput = document.getElementById("seedbox-host") as HTMLInputElement;
const seedboxPortInput = document.getElementById("seedbox-port") as HTMLInputElement;
const seedboxUsernameInput = document.getElementById("seedbox-username") as HTMLInputElement;
const seedboxPasswordInput = document.getElementById("seedbox-password") as HTMLInputElement;
const seedboxRemotePathInput = document.getElementById("seedbox-remote-path") as HTMLInputElement;
const seedboxMountPathInput = document.getElementById("seedbox-mount-path") as HTMLInputElement;
const seedboxDriveLetterInput = document.getElementById("seedbox-drive-letter") as HTMLInputElement;
const seedboxMountRow = document.getElementById("seedbox-mount-row") as HTMLDivElement;
const seedboxDriveRow = document.getElementById("seedbox-drive-row") as HTMLDivElement;
const seedboxReadOnlyCheckbox = document.getElementById("seedbox-read-only") as HTMLInputElement;
const seedboxAllowUnverifiedCheckbox = document.getElementById("seedbox-allow-unverified") as HTMLInputElement;
const seedboxHelp = document.getElementById("seedbox-help") as HTMLParagraphElement;
const btnTestSeedbox = document.getElementById("btn-test-seedbox") as HTMLButtonElement;
const btnForgetSeedbox = document.getElementById("btn-forget-seedbox") as HTMLButtonElement;
const btnBrowseSeedbox = document.getElementById("btn-browse-seedbox") as HTMLButtonElement;
const seedboxTestLoader = document.getElementById("seedbox-test-loader") as HTMLSpanElement;
const gdriveRemotePathInput = document.getElementById("gdrive-remote-path") as HTMLInputElement;
const gdriveRootFolderIdInput = document.getElementById("gdrive-root-folder-id") as HTMLInputElement;
const gdriveHelp = document.getElementById("gdrive-help") as HTMLParagraphElement;
const btnConnectGdrive = document.getElementById("btn-connect-gdrive") as HTMLButtonElement;
const btnTestGdrive = document.getElementById("btn-test-gdrive") as HTMLButtonElement;
const gdriveTestLoader = document.getElementById("gdrive-test-loader") as HTMLSpanElement;
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
  b2Panel.classList.toggle("hidden", provider !== "B2");
  gdrivePanel.classList.toggle("hidden", provider !== "GoogleDrive");
  seedboxPanel.classList.toggle("hidden", provider !== "Seedbox");
  scheduleFitWindow();
}

function normalizeSeedboxHostInUi() {
  let host = seedboxHostInput.value.trim();
  for (const scheme of ["https://", "http://", "ftps://", "ftp://"]) {
    if (host.toLowerCase().startsWith(scheme)) {
      host = host.slice(scheme.length).trim();
      break;
    }
  }
  while (host.endsWith("/")) {
    host = host.slice(0, -1);
  }
  if (seedboxHostInput.value !== host) {
    seedboxHostInput.value = host;
  }
}

function readSeedboxSettings(): SeedboxSettings {
  const port = Number.parseInt(seedboxPortInput.value.trim(), 10) || 21;
  const mountPath = seedboxMountPathInput.value.trim();
  return {
    remoteName: "seedbox",
    host: seedboxHostInput.value.trim(),
    username: seedboxUsernameInput.value.trim(),
    port,
    remotePath: seedboxRemotePathInput.value.trim(),
    mountPath: mountPath || (platform === "macos" ? DEFAULT_SEEDBOX_MOUNT_PATH : ""),
    driveLetter: seedboxDriveLetterInput.value.trim() || "S",
    allowUnverifiedCertificate: seedboxAllowUnverifiedCheckbox.checked,
    readOnly: seedboxReadOnlyCheckbox.checked,
  };
}

function refreshSeedboxConnectionUi() {
  btnForgetSeedbox.classList.toggle("hidden", !seedboxConfigured);
  seedboxTestLoader.classList.add("hidden");
  seedboxHelp.textContent = seedboxConfigured
    ? "Seedbox FTPS is configured. Use Save and Mount All to mount it."
    : "Use your Ultra.cc FTP/SFTP connection details. Host is usually your server name, port is 21, and Remote Folder is usually downloads.";
}

function configureSeedboxPlatformUi() {
  const isMac = platform === "macos";
  seedboxMountRow.classList.toggle("hidden", !isMac);
  seedboxDriveRow.classList.toggle("hidden", isMac);
  btnBrowseSeedbox.classList.toggle("hidden", !isMac);
}

function readGoogleDriveSettings(): GoogleDriveSettings {
  return {
    remoteName: "gdrive",
    remotePath: gdriveRemotePathInput.value.trim(),
    rootFolderId: gdriveRootFolderIdInput.value.trim(),
    mountPath: "",
    driveLetter: "G",
  };
}

function refreshGoogleDriveConnectionUi() {
  btnConnectGdrive.textContent = googleDriveConnected
    ? "Disconnect Google Drive"
    : "Connect Google Drive";
  btnTestGdrive.classList.toggle("hidden", !googleDriveConnected);
  gdriveTestLoader.classList.add("hidden");
  gdriveHelp.classList.toggle("hidden", googleDriveConnected);
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
    googleDrive: readGoogleDriveSettings(),
    seedbox: readSeedboxSettings(),
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
    gdriveHelp.textContent =
      "Click Connect Google Drive and sign in through the browser. After it connects, use Save and Mount All. Google Drive mounts to G:.";
  } else {
    gdriveHelp.textContent =
      "Click Connect Google Drive and sign in through the browser. After it connects, use Save and Mount All. Google Drive will mount as a disk named Google Drive under ~/Drives/Google Drive.";
  }

  const loaded = await invoke<LoadedSettings>("load_settings_cmd");
  providerSelect.value = loaded.settings.selectedProvider;
  keyIdInput.value = loaded.applicationKeyId;
  keyInput.value = loaded.applicationKey;
  gdriveRemotePathInput.value = loaded.settings.googleDrive?.remotePath ?? "";
  gdriveRootFolderIdInput.value = loaded.settings.googleDrive?.rootFolderId ?? "";

  const seedbox = loaded.settings.seedbox;
  seedboxHostInput.value = seedbox?.host ?? "";
  seedboxUsernameInput.value = seedbox?.username ?? "";
  seedboxPortInput.value = String(seedbox?.port ?? 21);
  seedboxRemotePathInput.value = seedbox?.remotePath ?? "downloads";
  seedboxMountPathInput.value = seedbox?.mountPath ?? "";
  seedboxDriveLetterInput.value = seedbox?.driveLetter ?? "S";
  seedboxReadOnlyCheckbox.checked = seedbox?.readOnly ?? true;
  seedboxAllowUnverifiedCheckbox.checked = seedbox?.allowUnverifiedCertificate ?? true;
  if (platform === "macos" && !seedboxMountPathInput.value.trim()) {
    seedboxMountPathInput.value = DEFAULT_SEEDBOX_MOUNT_PATH;
  }

  startAtLoginCheckbox.checked = loaded.settings.startAtLogin;
  startMinimizedCheckbox.checked = loaded.settings.startMinimized;
  renderBuckets(loaded.settings.buckets);
  googleDriveConnected = loaded.isGoogleDriveConfigured;
  seedboxConfigured = loaded.isSeedboxConfigured;
  configureSeedboxPlatformUi();
  refreshGoogleDriveConnectionUi();
  refreshSeedboxConnectionUi();
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

    if (settings.selectedProvider === "B2") {
      if (keyIdInput.value.trim() || keyInput.value.trim()) {
        await invoke("save_b2_credentials_cmd", {
          credentials: {
            applicationKeyId: keyIdInput.value.trim(),
            applicationKey: keyInput.value.trim(),
          },
        });
      }
    }

    await invoke("mount_all", {
      request: {
        applicationKeyId: keyIdInput.value.trim(),
        applicationKey: keyInput.value.trim(),
        buckets: settings.buckets,
        googleDrive: settings.googleDrive,
        seedbox: settings.seedbox,
        seedboxPassword: seedboxPasswordInput.value,
        selectedProvider: settings.selectedProvider,
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

btnConnectGdrive.addEventListener("click", async () => {
  const settings = collectSettings();
  settings.selectedProvider = "GoogleDrive";
  providerSelect.value = "GoogleDrive";
  updateProviderPanels();

  try {
    await invoke("save_settings_cmd", { settings });
    btnConnectGdrive.disabled = true;
    btnTestGdrive.disabled = true;
    btnConnectGdrive.textContent = googleDriveConnected ? "Disconnecting..." : "Connecting...";

    if (googleDriveConnected) {
      await invoke("disconnect_google_drive_cmd", {
        googleDrive: settings.googleDrive,
      });
      googleDriveConnected = false;
      appendLog({
        level: "INFO",
        message: "Google Drive is disconnected.",
        timestamp: new Date().toLocaleTimeString(),
      });
    } else {
      appendLog({
        level: "INFO",
        message: "Starting Google Drive authorization. Complete the sign-in in your browser.",
        timestamp: new Date().toLocaleTimeString(),
      });
      await invoke("configure_google_drive_cmd", {
        googleDrive: settings.googleDrive,
      });
      googleDriveConnected = true;
      appendLog({
        level: "INFO",
        message: "Google Drive is connected. You can now click Save and Mount All.",
        timestamp: new Date().toLocaleTimeString(),
      });
    }
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: new Date().toLocaleTimeString(),
    });
  } finally {
    refreshGoogleDriveConnectionUi();
    btnConnectGdrive.disabled = false;
    btnTestGdrive.disabled = false;
  }
});

btnTestGdrive.addEventListener("click", async () => {
  if (btnTestGdrive.disabled) {
    return;
  }

  const settings = collectSettings();
  btnTestGdrive.disabled = true;
  gdriveTestLoader.classList.remove("hidden");

  try {
    await invoke("save_settings_cmd", { settings });
    await invoke("test_google_drive_connection_cmd", {
      googleDrive: settings.googleDrive,
    });
    appendLog({
      level: "INFO",
      message: "Google Drive connection test succeeded.",
      timestamp: new Date().toLocaleTimeString(),
    });
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: new Date().toLocaleTimeString(),
    });
  } finally {
    gdriveTestLoader.classList.add("hidden");
    btnTestGdrive.disabled = false;
  }
});

gdriveRemotePathInput.addEventListener("change", () => {
  void savePreferences();
});

gdriveRootFolderIdInput.addEventListener("change", () => {
  void savePreferences();
});

btnTestSeedbox.addEventListener("click", async () => {
  if (btnTestSeedbox.disabled) {
    return;
  }

  normalizeSeedboxHostInUi();
  const settings = collectSettings();
  btnTestSeedbox.disabled = true;
  btnForgetSeedbox.disabled = true;
  seedboxTestLoader.classList.remove("hidden");

  try {
    await invoke("save_settings_cmd", { settings });
    await invoke("test_seedbox_connection_cmd", {
      seedbox: settings.seedbox,
      password: seedboxPasswordInput.value,
    });
    if (seedboxPasswordInput.value) {
      seedboxPasswordInput.value = "";
    }
    seedboxConfigured = true;
    appendLog({
      level: "INFO",
      message: "Seedbox connection test succeeded.",
      timestamp: new Date().toLocaleTimeString(),
    });
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: new Date().toLocaleTimeString(),
    });
  } finally {
    seedboxTestLoader.classList.add("hidden");
    btnTestSeedbox.disabled = false;
    btnForgetSeedbox.disabled = false;
    refreshSeedboxConnectionUi();
  }
});

btnForgetSeedbox.addEventListener("click", async () => {
  const settings = collectSettings();

  try {
    await invoke("save_settings_cmd", { settings });
    await invoke("forget_seedbox_cmd", { seedbox: settings.seedbox });
    seedboxPasswordInput.value = "";
    seedboxConfigured = false;
    appendLog({
      level: "INFO",
      message: "Seedbox is disconnected.",
      timestamp: new Date().toLocaleTimeString(),
    });
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: new Date().toLocaleTimeString(),
    });
  } finally {
    refreshSeedboxConnectionUi();
  }
});

btnBrowseSeedbox.addEventListener("click", async () => {
  try {
    const selected = await invoke<string | null>("browse_folder");
    if (selected) {
      seedboxMountPathInput.value = selected;
      void savePreferences();
    }
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: new Date().toLocaleTimeString(),
    });
  }
});

for (const el of [
  seedboxHostInput,
  seedboxPortInput,
  seedboxUsernameInput,
  seedboxRemotePathInput,
  seedboxMountPathInput,
  seedboxDriveLetterInput,
  seedboxReadOnlyCheckbox,
  seedboxAllowUnverifiedCheckbox,
]) {
  el.addEventListener("change", () => {
    if (el === seedboxHostInput) {
      normalizeSeedboxHostInUi();
    }
    void savePreferences();
  });
}

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
