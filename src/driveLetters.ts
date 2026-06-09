export const RESERVED_WINDOWS_DRIVE_LETTERS = ["G", "S"];

export function normalizeDriveLetter(value: string): string {
  return value.trim().replace(/:$/, "").toUpperCase();
}

export function nextAvailableWindowsDriveLetter(usedLetters: Iterable<string>): string {
  const used = new Set(
    [...usedLetters]
      .map(normalizeDriveLetter)
      .filter((letter) => /^[A-Z]$/.test(letter)),
  );

  for (let code = "Z".charCodeAt(0); code >= "A".charCodeAt(0); code -= 1) {
    const letter = String.fromCharCode(code);
    if (!used.has(letter)) {
      return letter;
    }
  }

  return "";
}
