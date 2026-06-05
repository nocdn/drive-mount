import type { AppSettings, LogLine } from "./types";
import { formatLogTimestamp } from "./logs";

interface ValidationElements {
  bucketsList: HTMLDivElement;
  keyIdInput: HTMLInputElement;
  keyInput: HTMLInputElement;
  seedboxHostInput: HTMLInputElement;
  seedboxPortInput: HTMLInputElement;
  seedboxUsernameInput: HTMLInputElement;
  seedboxPasswordInput: HTMLInputElement;
}

interface ValidationState {
  platform(): string;
  hasSavedB2Credentials(): boolean;
  hasSavedSeedboxPassword(): boolean;
  appendLog(line: LogLine): void;
  normalizeSeedboxHostInUi(): void;
}

export interface ValidationController {
  parseSeedboxPort(): number;
  validateB2ForMount(settings: AppSettings): boolean;
  validateSeedboxForConnection(requirePassword: boolean): boolean;
}

export function createValidationController(
  elements: ValidationElements,
  state: ValidationState,
): ValidationController {
  function ensureFieldErrorElement(input: HTMLElement): HTMLParagraphElement {
    const existing = input.parentElement?.querySelector<HTMLParagraphElement>(".field-error");
    if (existing) {
      return existing;
    }

    const error = document.createElement("p");
    error.className = "field-error hidden";
    input.insertAdjacentElement("afterend", error);
    return error;
  }

  function setFieldError(input: HTMLInputElement, message: string | null) {
    const error = ensureFieldErrorElement(input);
    input.classList.toggle("invalid", Boolean(message));
    error.textContent = message ?? "";
    error.classList.toggle("hidden", !message);
  }

  function clearFieldError(input: HTMLInputElement) {
    setFieldError(input, null);
  }

  function appendValidationLog(message: string) {
    state.appendLog({
      level: "ERROR",
      message,
      timestamp: formatLogTimestamp(),
    });
  }

  function isSingleFolderName(value: string): boolean {
    return value.trim() !== "" && !/[\\/]/.test(value) && value !== "." && value !== "..";
  }

  function normalizedDriveLetter(value: string): string {
    return value.trim().replace(/:$/, "").toUpperCase();
  }

  function validateDriveLetter(input: HTMLInputElement, reserved: Set<string>): boolean {
    const letter = normalizedDriveLetter(input.value);
    if (!/^[A-Z]$/.test(letter)) {
      setFieldError(input, "Use a single drive letter A-Z.");
      return false;
    }
    if (reserved.has(letter)) {
      setFieldError(input, `${letter}: is reserved.`);
      return false;
    }
    clearFieldError(input);
    input.value = letter;
    return true;
  }

  function parseSeedboxPort(): number {
    const raw = elements.seedboxPortInput.value.trim();
    if (raw === "") {
      return 21;
    }
    if (!/^\d+$/.test(raw)) {
      return 0;
    }
    return Number(raw);
  }

  function validateSeedboxForConnection(requirePassword: boolean): boolean {
    let ok = true;
    state.normalizeSeedboxHostInUi();

    const host = elements.seedboxHostInput.value.trim();
    if (!host || /[\\/]/.test(host)) {
      setFieldError(elements.seedboxHostInput, "Enter a host name without a path.");
      ok = false;
    } else {
      clearFieldError(elements.seedboxHostInput);
    }

    if (!elements.seedboxUsernameInput.value.trim()) {
      setFieldError(elements.seedboxUsernameInput, "Enter the Seedbox username.");
      ok = false;
    } else {
      clearFieldError(elements.seedboxUsernameInput);
    }

    const port = parseSeedboxPort();
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      setFieldError(elements.seedboxPortInput, "Use a port from 1 to 65535.");
      ok = false;
    } else {
      clearFieldError(elements.seedboxPortInput);
      elements.seedboxPortInput.value = String(port);
    }

    if (
      requirePassword &&
      !elements.seedboxPasswordInput.value.trim() &&
      !state.hasSavedSeedboxPassword()
    ) {
      setFieldError(elements.seedboxPasswordInput, "Enter the FTPS password.");
      ok = false;
    } else {
      clearFieldError(elements.seedboxPasswordInput);
    }

    if (!ok) {
      appendValidationLog("Fix the highlighted Seedbox fields before continuing.");
    }
    return ok;
  }

  function validateB2ForMount(settings: AppSettings): boolean {
    let ok = true;
    const buckets = settings.buckets.filter((bucket) => bucket.bucketName.trim());
    const seenBuckets = new Set<string>();
    const seenDrives = new Set<string>();

    elements.bucketsList.querySelectorAll<HTMLElement>(".bucket-row").forEach((row) => {
      const bucketInput = row.querySelector<HTMLInputElement>(".bucket-name-input");
      const driveInput = row.querySelector<HTMLInputElement>(".bucket-drive-input");
      if (!bucketInput) {
        return;
      }

      const bucketName = bucketInput.value.trim();
      if (!bucketName) {
        clearFieldError(bucketInput);
        return;
      }
      if (!isSingleFolderName(bucketName)) {
        setFieldError(bucketInput, "Use one bucket/folder name, without slashes.");
        ok = false;
      } else {
        const key = bucketName.toLowerCase();
        if (seenBuckets.has(key)) {
          setFieldError(bucketInput, "This bucket is already listed.");
          ok = false;
        } else {
          seenBuckets.add(key);
          clearFieldError(bucketInput);
        }
      }

      if (state.platform() !== "macos" && driveInput) {
        if (validateDriveLetter(driveInput, new Set(["G", "S"]))) {
          const letter = normalizedDriveLetter(driveInput.value);
          if (seenDrives.has(letter)) {
            setFieldError(driveInput, "This drive is already used.");
            ok = false;
          } else {
            seenDrives.add(letter);
          }
        } else {
          ok = false;
        }
      }
    });

    const keyId = elements.keyIdInput.value.trim();
    const key = elements.keyInput.value.trim();
    if ((keyId || key) && (!keyId || !key)) {
      if (!keyId) {
        setFieldError(elements.keyIdInput, "Enter the key ID.");
      } else {
        clearFieldError(elements.keyIdInput);
      }
      if (!key) {
        setFieldError(elements.keyInput, "Enter the application key.");
      } else {
        clearFieldError(elements.keyInput);
      }
      ok = false;
    } else if (buckets.length > 0) {
      if (!keyId && !key && !state.hasSavedB2Credentials()) {
        setFieldError(elements.keyIdInput, "Enter B2 credentials or save them first.");
        setFieldError(elements.keyInput, "Enter B2 credentials or save them first.");
        ok = false;
      } else {
        clearFieldError(elements.keyIdInput);
        clearFieldError(elements.keyInput);
      }
    } else {
      clearFieldError(elements.keyIdInput);
      clearFieldError(elements.keyInput);
    }

    if (!ok) {
      appendValidationLog("Fix the highlighted B2 fields before mounting.");
    }
    return ok;
  }

  return {
    parseSeedboxPort,
    validateB2ForMount,
    validateSeedboxForConnection,
  };
}
