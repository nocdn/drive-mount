import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isEnabled, enable, disable } from "@tauri-apps/plugin-autostart";
import { createCopiedButtonFeedback } from "./copyButtonFeedback";
import {
  nextAvailableWindowsDriveLetter,
  normalizeDriveLetter,
  RESERVED_WINDOWS_DRIVE_LETTERS,
} from "./driveLetters";
import { createLogController, formatLogTimestamp } from "./logs";
import {
  hasMountRelevantSettingsChanges,
  mountRelevantSettingsSnapshot,
} from "./mountSettings";
import type {
  AppSettings,
  BucketMount,
  CloudProvider,
  GoogleDriveSettings,
  LoadedCredentials,
  LogLine,
  MountOperation,
  MountState,
  OneDriveSettings,
  SeedboxSettings,
} from "./types";
import { createValidationController } from "./validation";
import {
  installVisibilityWindowFit,
  scheduleFitWindow,
  startWindowFitWatcher,
} from "./windowSizing";

let platform = "macos";
let mounted = false;
let googleDriveConnected = false;
let oneDriveConnected = false;
let seedboxConfigured = false;
let hasSavedB2Credentials = false;
let hasSavedSeedboxPassword = false;
let mountOperation: MountOperation = null;
let activeMountSettingsSnapshot: string | null = null;
let unavailableWindowsDriveLetters = new Set<string>();

const providerSelect = document.getElementById("provider") as HTMLSelectElement;
const b2Panel = document.getElementById("b2-panel") as HTMLDivElement;
const gdrivePanel = document.getElementById("gdrive-panel") as HTMLDivElement;
const onedrivePanel = document.getElementById("onedrive-panel") as HTMLDivElement;
const seedboxPanel = document.getElementById("seedbox-panel") as HTMLDivElement;
const seedboxHostInput = document.getElementById("seedbox-host") as HTMLInputElement;
const seedboxPortInput = document.getElementById("seedbox-port") as HTMLInputElement;
const seedboxUsernameInput = document.getElementById("seedbox-username") as HTMLInputElement;
const seedboxPasswordInput = document.getElementById("seedbox-password") as HTMLInputElement;
const seedboxRemotePathInput = document.getElementById("seedbox-remote-path") as HTMLInputElement;
const seedboxReadOnlyCheckbox = document.getElementById("seedbox-read-only") as HTMLInputElement;
const seedboxAllowUnverifiedCheckbox = document.getElementById("seedbox-allow-unverified") as HTMLInputElement;
const seedboxHelp = document.getElementById("seedbox-help") as HTMLParagraphElement;
const btnTestSeedbox = document.getElementById("btn-test-seedbox") as HTMLButtonElement;
const btnForgetSeedbox = document.getElementById("btn-forget-seedbox") as HTMLButtonElement;
const gdriveRemotePathInput = document.getElementById("gdrive-remote-path") as HTMLInputElement;
const gdriveRootFolderIdInput = document.getElementById("gdrive-root-folder-id") as HTMLInputElement;
const gdriveHelp = document.getElementById("gdrive-help") as HTMLParagraphElement;
const btnConnectGdrive = document.getElementById("btn-connect-gdrive") as HTMLButtonElement;
const btnTestGdrive = document.getElementById("btn-test-gdrive") as HTMLButtonElement;
const onedriveRemotePathInput = document.getElementById("onedrive-remote-path") as HTMLInputElement;
const onedriveHelp = document.getElementById("onedrive-help") as HTMLParagraphElement;
const btnConnectOnedrive = document.getElementById("btn-connect-onedrive") as HTMLButtonElement;
const btnTestOnedrive = document.getElementById("btn-test-onedrive") as HTMLButtonElement;
const keyIdInput = document.getElementById("key-id") as HTMLInputElement;
const keyInput = document.getElementById("key") as HTMLInputElement;
const b2CredentialsStatus = document.getElementById("b2-credentials-status") as HTMLParagraphElement;
const bucketsList = document.getElementById("buckets-list") as HTMLDivElement;
const bucketsHelp = document.getElementById("buckets-help") as HTMLParagraphElement;
const addBucketBtn = document.getElementById("add-bucket") as HTMLButtonElement;
const startAtLoginCheckbox = document.getElementById("start-at-login") as HTMLInputElement;
const startMinimizedCheckbox = document.getElementById("start-minimized") as HTMLInputElement;
const startMinimizedLabel = document.getElementById("start-minimized-label") as HTMLSpanElement;
const btnMount = document.getElementById("btn-mount") as HTMLButtonElement;
const btnUnmount = document.getElementById("btn-unmount") as HTMLButtonElement;
const btnRefreshCaches = document.getElementById("btn-refresh-caches") as HTMLButtonElement;
const btnClearLogs = document.getElementById("btn-clear-logs") as HTMLButtonElement;
const btnCopyLogs = document.getElementById("btn-copy-logs") as HTMLButtonElement;
const btnRestart = document.getElementById("btn-restart") as HTMLButtonElement;
const logsEl = document.getElementById("logs") as HTMLPreElement;
const { appendLog, clearRenderedLogs, flushPendingLogLines } = createLogController(logsEl);
const showCopyLogsFeedback = createCopiedButtonFeedback(btnCopyLogs);
const { parseSeedboxPort, validateB2ForMount, validateSeedboxForConnection } =
  createValidationController(
    {
      bucketsList,
      keyIdInput,
      keyInput,
      seedboxHostInput,
      seedboxPortInput,
      seedboxUsernameInput,
      seedboxPasswordInput,
    },
    {
      platform: () => platform,
      hasSavedB2Credentials: () => hasSavedB2Credentials,
      hasSavedSeedboxPassword: () => hasSavedSeedboxPassword,
      appendLog,
      normalizeSeedboxHostInUi,
    },
  );
const mountButtonLabel = btnMount.textContent ?? "Save and Mount All";
const unmountButtonLabel = btnUnmount.textContent ?? "Unmount All";
const refreshCachesButtonLabel = btnRefreshCaches.textContent ?? "Refresh";
const restartButtonLabel = btnRestart.textContent ?? "Restart Mounts";

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

function waitForUiFeedback() {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve());
    });
  });
}

function setMountOperation(operation: MountOperation) {
  mountOperation = operation;
  updateMountButtons();
}

function updateMountButtons() {
  btnMount.textContent = mountOperation === "mounting" ? "Mounting" : mountButtonLabel;
  btnUnmount.textContent =
    mountOperation === "unmounting" || mountOperation === "restarting" ? "Unmounting..." : unmountButtonLabel;
  btnRefreshCaches.textContent = mountOperation === "refreshing" ? "Refreshing..." : refreshCachesButtonLabel;
  btnRestart.textContent = mountOperation === "restarting" ? "Restarting Mounts..." : restartButtonLabel;
  btnRefreshCaches.disabled = mountOperation !== null || !mounted;
  btnRestart.disabled = mountOperation !== null;

  if (mountOperation) {
    btnMount.disabled = true;
    btnUnmount.disabled = true;
    return;
  }

  const hasChangedMountedSettings = hasMountRelevantSettingsChanges(
    collectSettings(),
    activeMountSettingsSnapshot,
    platform,
  );
  btnMount.disabled = mounted && !hasChangedMountedSettings;
  btnUnmount.disabled = !mounted;
  btnRefreshCaches.disabled = !mounted;
}

function renderMountState(state: MountState) {
  mounted = state.mounted;
  if (mounted) {
    activeMountSettingsSnapshot ??= mountRelevantSettingsSnapshot(collectSettings(), platform);
  } else {
    activeMountSettingsSnapshot = null;
  }
  updateMountButtons();
  scheduleFitWindow();
}

function updateProviderPanels() {
  const provider = providerSelect.value as CloudProvider;
  b2Panel.classList.toggle("hidden", provider !== "B2");
  gdrivePanel.classList.toggle("hidden", provider !== "GoogleDrive");
  onedrivePanel.classList.toggle("hidden", provider !== "OneDrive");
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
  return {
    host: seedboxHostInput.value.trim(),
    username: seedboxUsernameInput.value.trim(),
    port: parseSeedboxPort(),
    remotePath: seedboxRemotePathInput.value.trim(),
    allowUnverifiedCertificate: seedboxAllowUnverifiedCheckbox.checked,
    readOnly: seedboxReadOnlyCheckbox.checked,
  };
}

function refreshSeedboxConnectionUi() {
  btnForgetSeedbox.classList.toggle("hidden", !seedboxConfigured && !hasSavedSeedboxPassword);
  seedboxHelp.textContent = seedboxConfigured || hasSavedSeedboxPassword
    ? platform === "macos"
      ? "Seedbox FTPS is configured. Use Save and Mount All to mount it at ~/Drives/seedbox."
      : "Seedbox FTPS is configured. Use Save and Mount All to mount it as S: named seedbox."
    : "Use your Ultra.cc FTP/SFTP connection details. Host is usually your server name, port is 21, and Remote Folder is usually downloads.";
}

function configureSeedboxPlatformUi() {
  scheduleFitWindow();
}

function readGoogleDriveSettings(): GoogleDriveSettings {
  return {
    remotePath: gdriveRemotePathInput.value.trim(),
    rootFolderId: gdriveRootFolderIdInput.value.trim(),
  };
}

function refreshGoogleDriveConnectionUi() {
  btnConnectGdrive.textContent = googleDriveConnected
    ? "Disconnect Google Drive"
    : "Connect Google Drive";
  btnTestGdrive.classList.toggle("hidden", !googleDriveConnected);
  gdriveHelp.classList.toggle("hidden", googleDriveConnected);
}

function readOneDriveSettings(): OneDriveSettings {
  return {
    remotePath: onedriveRemotePathInput.value.trim(),
  };
}

function refreshOneDriveConnectionUi() {
  btnConnectOnedrive.textContent = oneDriveConnected
    ? "Disconnect OneDrive"
    : "Connect OneDrive";
  btnTestOnedrive.classList.toggle("hidden", !oneDriveConnected);
  onedriveHelp.classList.toggle("hidden", oneDriveConnected);
}

function collectVisibleBucketDriveLetters(excludedInput?: HTMLInputElement): string[] {
  return [...bucketsList.querySelectorAll<HTMLInputElement>(".bucket-drive-input")]
    .filter((input) => input !== excludedInput)
    .map((input) => input.value);
}

function defaultBucketDriveLetter(
  extraUsedLetters: Iterable<string> = [],
  excludedInput?: HTMLInputElement,
): string {
  if (platform === "macos") {
    return "";
  }

  return nextAvailableWindowsDriveLetter([
    ...RESERVED_WINDOWS_DRIVE_LETTERS,
    ...unavailableWindowsDriveLetters,
    ...collectVisibleBucketDriveLetters(excludedInput),
    ...extraUsedLetters,
  ]);
}

function createBucketRow(bucket?: BucketMount) {
  const rowBucket = bucket ?? { bucketName: "", driveLetter: defaultBucketDriveLetter() };
  const row = document.createElement("div");
  row.className = "bucket-row";

  const nameLabel = document.createElement("label");
  nameLabel.textContent = "Bucket";
  const nameInput = document.createElement("input");
  nameInput.type = "text";
  nameInput.className = "bucket-name-input";
  nameInput.autocomplete = "off";
  nameInput.spellcheck = false;
  nameInput.setAttribute("autocorrect", "off");
  nameInput.setAttribute("autocapitalize", "none");
  nameInput.value = rowBucket.bucketName;
  nameInput.addEventListener("input", updateMountButtons);

  const bucketGroup = document.createElement("div");
  bucketGroup.className = "bucket-inline-group";
  bucketGroup.append(nameLabel, nameInput);
  row.appendChild(bucketGroup);

  let driveInput: HTMLInputElement | null = null;

  if (platform !== "macos") {
    const driveLabel = document.createElement("label");
    driveLabel.textContent = "Drive";
    driveInput = document.createElement("input");
    driveInput.type = "text";
    driveInput.className = "bucket-drive-input";
    driveInput.maxLength = 3;
    driveInput.value = rowBucket.driveLetter || defaultBucketDriveLetter();
    driveInput.addEventListener("input", updateMountButtons);

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
      if (driveInput) driveInput.value = defaultBucketDriveLetter([], driveInput);
      updateMountButtons();
      return;
    }
    row.remove();
    updateMountButtons();
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
        driveLetter: "",
      });
    } else {
      buckets.push({
        bucketName,
        driveLetter: second,
      });
    }
  });
  return buckets;
}

function renderBuckets(buckets: BucketMount[]) {
  bucketsList.innerHTML = "";
  const rows = buckets.length > 0 ? buckets : [{ bucketName: "", driveLetter: defaultBucketDriveLetter() }];
  rows.forEach((bucket) => createBucketRow(bucket));
  scheduleFitWindow();
}

async function refreshUnavailableWindowsDriveLetters() {
  if (platform !== "windows") {
    unavailableWindowsDriveLetters = new Set();
    return;
  }

  try {
    const letters = await invoke<string[]>("used_windows_drive_letters_cmd");
    unavailableWindowsDriveLetters = new Set(letters.map(normalizeDriveLetter));
  } catch (err) {
    unavailableWindowsDriveLetters = new Set();
    appendLog({
      level: "ERROR",
      message: `Could not check Windows drive letters: ${String(err)}`,
      timestamp: formatLogTimestamp(),
    });
  }
}

function collectSettings(): AppSettings {
  return {
    selectedProvider: providerSelect.value as CloudProvider,
    buckets: collectBuckets(),
    googleDrive: readGoogleDriveSettings(),
    oneDrive: readOneDriveSettings(),
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
      timestamp: formatLogTimestamp(),
    });
  }
}

async function savePreferences() {
  const settings = collectSettings();
  await invoke("save_settings_cmd", { settings });
  await applyAutostart(settings.startAtLogin);
}

function configurePlatformUi() {
  if (platform === "windows") {
    bucketsHelp.textContent = "Each bucket is mounted directly to its own drive letter.";
    startMinimizedLabel.textContent = "Start minimized to tray";
    gdriveHelp.textContent =
      "Click Connect Google Drive and sign in through the browser. After it connects, use Save and Mount All. Google Drive mounts to G: named google-drive.";
    onedriveHelp.textContent =
      "Click Connect OneDrive and sign in through the browser. After it connects, use Save and Mount All. OneDrive mounts to O: named onedrive.";
  } else {
    bucketsHelp.textContent = "Each bucket mounts as a folder under ~/Drives.";
    gdriveHelp.textContent =
      "Click Connect Google Drive and sign in through the browser. After it connects, use Save and Mount All. Google Drive mounts as a folder at ~/Drives/google-drive.";
    onedriveHelp.textContent =
      "Click Connect OneDrive and sign in through the browser. After it connects, use Save and Mount All. OneDrive mounts as a folder at ~/Drives/onedrive.";
  }
}

function renderSettings(settings: AppSettings) {
  providerSelect.value = settings.selectedProvider;
  keyIdInput.placeholder = "";
  keyInput.placeholder = "";
  keyIdInput.value = "";
  keyInput.value = "";
  b2CredentialsStatus.textContent = "Unlocking saved credentials...";
  gdriveRemotePathInput.value = settings.googleDrive?.remotePath ?? "";
  gdriveRootFolderIdInput.value = settings.googleDrive?.rootFolderId ?? "";
  onedriveRemotePathInput.value = settings.oneDrive?.remotePath ?? "";

  const seedbox = settings.seedbox;
  seedboxHostInput.value = seedbox?.host ?? "";
  seedboxUsernameInput.value = seedbox?.username ?? "";
  seedboxPortInput.value = String(seedbox?.port ?? 21);
  seedboxRemotePathInput.value = seedbox?.remotePath ?? "downloads";
  seedboxReadOnlyCheckbox.checked = seedbox?.readOnly ?? true;
  seedboxAllowUnverifiedCheckbox.checked = seedbox?.allowUnverifiedCertificate ?? true;
  startAtLoginCheckbox.checked = settings.startAtLogin;
  startMinimizedCheckbox.checked = settings.startMinimized;
  renderBuckets(settings.buckets);
  configureSeedboxPlatformUi();
  refreshGoogleDriveConnectionUi();
  refreshOneDriveConnectionUi();
  refreshSeedboxConnectionUi();
  updateProviderPanels();
}

function renderCredentialState(loaded: LoadedCredentials) {
  hasSavedB2Credentials = loaded.hasSavedCredentials;
  hasSavedSeedboxPassword = loaded.hasSavedSeedboxPassword;
  keyIdInput.value = loaded.b2Credentials?.applicationKeyId ?? "";
  keyInput.value = loaded.b2Credentials?.applicationKey ?? "";
  keyIdInput.placeholder = "";
  keyInput.placeholder = "";
  b2CredentialsStatus.textContent = loaded.hasSavedCredentials
    ? "B2 credentials are saved securely and loaded into the fields."
    : "B2 credentials are stored securely after a successful save or mount.";
  googleDriveConnected = loaded.isGoogleDriveConfigured;
  oneDriveConnected = loaded.isOneDriveConfigured;
  seedboxConfigured = loaded.isSeedboxConfigured;
  refreshGoogleDriveConnectionUi();
  refreshOneDriveConnectionUi();
  refreshSeedboxConnectionUi();
  scheduleFitWindow();
}

async function unlockCredentialsAndAutoMount() {
  try {
    const loaded = await invoke<LoadedCredentials>("load_credentials_cmd");
    renderCredentialState(loaded);
    setMountOperation("mounting");
    await waitForUiFeedback();
    await invoke("attempt_auto_mount_cmd");
    mounted = await invoke<boolean>("is_mounted");
    renderMountState({ mounted });
  } catch (err) {
    b2CredentialsStatus.textContent =
      "Saved credentials could not be unlocked. Enter credentials manually to mount.";
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: formatLogTimestamp(),
    });
  } finally {
    setMountOperation(null);
  }
}

async function loadUi() {
  platform = await invoke<string>("get_platform");
  configurePlatformUi();
  await refreshUnavailableWindowsDriveLetters();

  const settings = await invoke<AppSettings>("load_settings_cmd");
  renderSettings(settings);
  activeMountSettingsSnapshot = mountRelevantSettingsSnapshot(settings, platform);

  mounted = await invoke<boolean>("is_mounted");
  renderMountState({ mounted });

  const fuseOk = await invoke<boolean>("is_fuse_installed_cmd");
  if (!fuseOk) {
    appendLog({
      level: "ERROR",
      message:
        platform === "macos"
          ? "macFUSE is not installed or has not been enabled."
          : "WinFsp is not installed.",
      timestamp: formatLogTimestamp(),
    });
  }

  appendLog({
    level: "INFO",
    message: "Cloud Drive Mount started. Use the tray icon to reopen Settings after closing this window.",
    timestamp: formatLogTimestamp(),
  });

  startWindowFitWatcher();
  await waitForUiFeedback();
  void applyAutostart(settings.startAtLogin);
  void unlockCredentialsAndAutoMount();
}

providerSelect.addEventListener("change", () => {
  updateProviderPanels();
});

addBucketBtn.addEventListener("click", () => {
  createBucketRow();
  updateMountButtons();
});

startAtLoginCheckbox.addEventListener("change", () => {
  void savePreferences();
});

startMinimizedCheckbox.addEventListener("change", () => {
  void savePreferences();
});

btnMount.addEventListener("click", async () => {
  if (
    mountOperation ||
    (mounted &&
      !hasMountRelevantSettingsChanges(collectSettings(), activeMountSettingsSnapshot, platform))
  ) {
    return;
  }

  setMountOperation("mounting");
  await waitForUiFeedback();

  try {
    const settings = collectSettings();
    const seedboxNeedsValidation =
      settings.seedbox.host.trim() !== "" || settings.seedbox.username.trim() !== "";
    if (!validateB2ForMount(settings)) {
      return;
    }
    if (seedboxNeedsValidation && !validateSeedboxForConnection(false)) {
      return;
    }

    await invoke("save_settings_cmd", { settings });
    await applyAutostart(settings.startAtLogin);

    if (keyIdInput.value.trim() || keyInput.value.trim()) {
      await invoke("save_b2_credentials_cmd", {
        credentials: {
          applicationKeyId: keyIdInput.value.trim(),
          applicationKey: keyInput.value.trim(),
        },
      });
      hasSavedB2Credentials = true;
      keyIdInput.placeholder = "";
      keyInput.placeholder = "";
      b2CredentialsStatus.textContent =
        "B2 credentials are saved securely and loaded into the fields.";
    }

    await invoke("mount_all", {
      request: {
        applicationKeyId: keyIdInput.value.trim(),
        applicationKey: keyInput.value.trim(),
        buckets: settings.buckets,
        googleDrive: settings.googleDrive,
        oneDrive: settings.oneDrive,
        seedbox: settings.seedbox,
        seedboxPassword: seedboxPasswordInput.value,
        selectedProvider: settings.selectedProvider,
      },
    });
    mounted = true;
    activeMountSettingsSnapshot = mountRelevantSettingsSnapshot(settings, platform);
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: formatLogTimestamp(),
    });
  } finally {
    setMountOperation(null);
  }
});

btnConnectGdrive.addEventListener("click", async () => {
  if (btnConnectGdrive.disabled) {
    return;
  }

  const settings = collectSettings();
  settings.selectedProvider = "GoogleDrive";
  providerSelect.value = "GoogleDrive";
  updateProviderPanels();
  btnConnectGdrive.disabled = true;
  btnTestGdrive.disabled = true;
  btnConnectGdrive.textContent = googleDriveConnected ? "Disconnecting..." : "Connecting...";

  try {
    await invoke("save_settings_cmd", { settings });

    if (googleDriveConnected) {
      await invoke("disconnect_google_drive_cmd", {
        googleDrive: settings.googleDrive,
      });
      googleDriveConnected = false;
      appendLog({
        level: "INFO",
        message: "Google Drive is disconnected.",
        timestamp: formatLogTimestamp(),
      });
    } else {
      appendLog({
        level: "INFO",
        message: "Starting Google Drive authorization. Complete the sign-in in your browser.",
        timestamp: formatLogTimestamp(),
      });
      await invoke("configure_google_drive_cmd", {
        googleDrive: settings.googleDrive,
      });
      googleDriveConnected = true;
      appendLog({
        level: "INFO",
        message: "Google Drive is connected. You can now click Save and Mount All.",
        timestamp: formatLogTimestamp(),
      });
    }
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: formatLogTimestamp(),
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

  try {
    await invoke("save_settings_cmd", { settings });
    await invoke("test_google_drive_connection_cmd", {
      googleDrive: settings.googleDrive,
    });
    appendLog({
      level: "INFO",
      message: "Google Drive connection test succeeded.",
      timestamp: formatLogTimestamp(),
    });
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: formatLogTimestamp(),
    });
  } finally {
    btnTestGdrive.disabled = false;
  }
});

btnConnectOnedrive.addEventListener("click", async () => {
  if (btnConnectOnedrive.disabled) {
    return;
  }

  const settings = collectSettings();
  settings.selectedProvider = "OneDrive";
  providerSelect.value = "OneDrive";
  updateProviderPanels();
  btnConnectOnedrive.disabled = true;
  btnTestOnedrive.disabled = true;
  btnConnectOnedrive.textContent = oneDriveConnected ? "Disconnecting..." : "Connecting...";

  try {
    await invoke("save_settings_cmd", { settings });

    if (oneDriveConnected) {
      await invoke("disconnect_one_drive_cmd", {
        oneDrive: settings.oneDrive,
      });
      oneDriveConnected = false;
      appendLog({
        level: "INFO",
        message: "OneDrive is disconnected.",
        timestamp: formatLogTimestamp(),
      });
    } else {
      appendLog({
        level: "INFO",
        message: "Starting OneDrive authorization. Complete the sign-in in your browser.",
        timestamp: formatLogTimestamp(),
      });
      await invoke("configure_one_drive_cmd", {
        oneDrive: settings.oneDrive,
      });
      oneDriveConnected = true;
      appendLog({
        level: "INFO",
        message: "OneDrive is connected. You can now click Save and Mount All.",
        timestamp: formatLogTimestamp(),
      });
    }
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: formatLogTimestamp(),
    });
  } finally {
    refreshOneDriveConnectionUi();
    btnConnectOnedrive.disabled = false;
    btnTestOnedrive.disabled = false;
  }
});

btnTestOnedrive.addEventListener("click", async () => {
  if (btnTestOnedrive.disabled) {
    return;
  }

  const settings = collectSettings();
  btnTestOnedrive.disabled = true;

  try {
    await invoke("save_settings_cmd", { settings });
    await invoke("test_one_drive_connection_cmd", {
      oneDrive: settings.oneDrive,
    });
    appendLog({
      level: "INFO",
      message: "OneDrive connection test succeeded.",
      timestamp: formatLogTimestamp(),
    });
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: formatLogTimestamp(),
    });
  } finally {
    btnTestOnedrive.disabled = false;
  }
});

gdriveRemotePathInput.addEventListener("change", () => {
  updateMountButtons();
  void savePreferences();
});

gdriveRootFolderIdInput.addEventListener("change", () => {
  updateMountButtons();
  void savePreferences();
});

onedriveRemotePathInput.addEventListener("change", () => {
  updateMountButtons();
  void savePreferences();
});

btnTestSeedbox.addEventListener("click", async () => {
  if (btnTestSeedbox.disabled) {
    return;
  }

  if (!validateSeedboxForConnection(true)) {
    return;
  }

  const settings = collectSettings();
  btnTestSeedbox.disabled = true;
  btnForgetSeedbox.disabled = true;

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
    hasSavedSeedboxPassword = true;
    appendLog({
      level: "INFO",
      message: "Seedbox connection test succeeded.",
      timestamp: formatLogTimestamp(),
    });
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: formatLogTimestamp(),
    });
  } finally {
    btnTestSeedbox.disabled = false;
    btnForgetSeedbox.disabled = false;
    refreshSeedboxConnectionUi();
  }
});

btnForgetSeedbox.addEventListener("click", async () => {
  if (btnForgetSeedbox.disabled) {
    return;
  }

  const settings = collectSettings();
  btnForgetSeedbox.disabled = true;
  btnTestSeedbox.disabled = true;

  try {
    await invoke("save_settings_cmd", { settings });
    await invoke("forget_seedbox_cmd", { seedbox: settings.seedbox });
    seedboxPasswordInput.value = "";
    seedboxConfigured = false;
    hasSavedSeedboxPassword = false;
    appendLog({
      level: "INFO",
      message: "Seedbox is disconnected.",
      timestamp: formatLogTimestamp(),
    });
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: formatLogTimestamp(),
    });
  } finally {
    refreshSeedboxConnectionUi();
    btnForgetSeedbox.disabled = false;
    btnTestSeedbox.disabled = false;
  }
});

for (const el of [
  seedboxHostInput,
  seedboxPortInput,
  seedboxUsernameInput,
  seedboxRemotePathInput,
  seedboxReadOnlyCheckbox,
  seedboxAllowUnverifiedCheckbox,
]) {
  el.addEventListener("change", () => {
    if (el === seedboxHostInput) {
      normalizeSeedboxHostInUi();
    }
    updateMountButtons();
    void savePreferences();
  });
}

btnUnmount.addEventListener("click", async () => {
  if (mountOperation || !mounted) {
    return;
  }

  setMountOperation("unmounting");
  await waitForUiFeedback();

  try {
    await invoke("unmount_all");
    mounted = false;
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: formatLogTimestamp(),
    });
  } finally {
    setMountOperation(null);
  }
});

btnRefreshCaches.addEventListener("click", async () => {
  if (mountOperation || !mounted) {
    return;
  }

  setMountOperation("refreshing");
  await waitForUiFeedback();

  try {
    await invoke("refresh_mount_caches");
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: formatLogTimestamp(),
    });
  } finally {
    setMountOperation(null);
  }
});

btnClearLogs.addEventListener("click", () => {
  clearRenderedLogs();
  void invoke("clear_logs");
});

btnCopyLogs.addEventListener("click", async () => {
  flushPendingLogLines();
  const text = logsEl.textContent ?? "";
  if (!text) {
    return;
  }
  try {
    await navigator.clipboard.writeText(text);
    showCopyLogsFeedback();
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: `Copy failed: ${String(err)}`,
      timestamp: formatLogTimestamp(),
    });
  }
});

btnRestart.addEventListener("click", async () => {
  if (mountOperation) {
    return;
  }

  setMountOperation("restarting");
  await waitForUiFeedback();

  try {
    await savePreferences();
    await invoke("restart_mounts");
  } catch (err) {
    appendLog({
      level: "ERROR",
      message: String(err),
      timestamp: formatLogTimestamp(),
    });
  } finally {
    setMountOperation(null);
  }
});

installVisibilityWindowFit();

void listen<LogLine>("log-line", (event) => {
  appendLog(event.payload);
});

void listen<MountState>("mount-state-changed", (event) => {
  renderMountState(event.payload);
});

void loadUi();
