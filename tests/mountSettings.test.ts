import { describe, expect, test } from "bun:test";
import {
  hasMountRelevantSettingsChanges,
  mountRelevantSettingsSnapshot,
} from "../src/mountSettings";
import type { AppSettings } from "../src/types";

function settings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    selectedProvider: "B2",
    buckets: [{ bucketName: "", driveLetter: "Z" }],
    googleDrive: { remotePath: "", rootFolderId: "" },
    seedbox: {
      host: "",
      username: "",
      port: 21,
      remotePath: "downloads",
      allowUnverifiedCertificate: true,
      readOnly: true,
    },
    startAtLogin: false,
    startMinimized: false,
    ...overrides,
  };
}

describe("mountRelevantSettingsSnapshot", () => {
  test("ignores newly added blank bucket rows", () => {
    const before = settings();
    const after = settings({
      buckets: [
        { bucketName: "", driveLetter: "Z" },
        { bucketName: "   ", driveLetter: "P" },
      ],
    });

    const snapshot = mountRelevantSettingsSnapshot(before, "macos");

    expect(hasMountRelevantSettingsChanges(after, snapshot, "macos")).toBe(false);
  });

  test("detects a newly typed bucket while mounted", () => {
    const before = settings();
    const after = settings({
      buckets: [
        { bucketName: "", driveLetter: "Z" },
        { bucketName: "nocdn-main", driveLetter: "P" },
      ],
    });

    const snapshot = mountRelevantSettingsSnapshot(before, "macos");

    expect(hasMountRelevantSettingsChanges(after, snapshot, "macos")).toBe(true);
  });

  test("does not treat login preferences as mount changes", () => {
    const before = settings();
    const after = settings({
      startAtLogin: true,
      startMinimized: true,
    });

    const snapshot = mountRelevantSettingsSnapshot(before, "macos");

    expect(hasMountRelevantSettingsChanges(after, snapshot, "macos")).toBe(false);
  });

  test("normalizes Windows drive letters for comparison", () => {
    const before = settings({
      buckets: [{ bucketName: "photos", driveLetter: "p" }],
    });
    const after = settings({
      buckets: [{ bucketName: " photos ", driveLetter: "P:" }],
    });

    const snapshot = mountRelevantSettingsSnapshot(before, "windows");

    expect(hasMountRelevantSettingsChanges(after, snapshot, "windows")).toBe(false);
  });
});
