import { describe, expect, test } from "bun:test";
import { targets, version } from "../scripts/download-rclone.mjs";

describe("rclone sidecar release", () => {
  test("includes the VFS handle caching race fix", () => {
    const [major, minor, patch] = version.slice(1).split(".").map(Number);
    expect(major > 1 || (major === 1 && (minor > 74 || (minor === 74 && patch >= 4)))).toBe(true);
  });

  test("pins matching archives and SHA-256 hashes for every supported platform", () => {
    expect(Object.keys(targets).sort()).toEqual([
      "aarch64-apple-darwin",
      "x86_64-apple-darwin",
      "x86_64-pc-windows-msvc",
    ]);
    for (const target of Object.values(targets)) {
      expect(target.archiveName).toStartWith(`rclone-${version}-`);
      expect(target.sha256).toMatch(/^[a-f0-9]{64}$/);
    }
  });
});
