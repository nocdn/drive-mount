import { describe, expect, test } from "bun:test";
import {
  nextAvailableWindowsDriveLetter,
  normalizeDriveLetter,
  RESERVED_WINDOWS_DRIVE_LETTERS,
} from "../src/driveLetters";

describe("nextAvailableWindowsDriveLetter", () => {
  test("reserves fixed provider drive letters", () => {
    expect(RESERVED_WINDOWS_DRIVE_LETTERS).toEqual(["G", "O", "S"]);
  });

  test("prefers Z when it is available", () => {
    expect(nextAvailableWindowsDriveLetter(RESERVED_WINDOWS_DRIVE_LETTERS)).toBe("Z");
  });

  test("skips reserved, configured, and system drive letters", () => {
    expect(
      nextAvailableWindowsDriveLetter([
        ...RESERVED_WINDOWS_DRIVE_LETTERS,
        "C",
        "Y",
        "z:",
      ]),
    ).toBe("X");
  });
});

describe("normalizeDriveLetter", () => {
  test("normalizes lowercase letters with optional colon", () => {
    expect(normalizeDriveLetter(" z: ")).toBe("Z");
  });
});
