export function createCopiedButtonFeedback(button: HTMLButtonElement, copiedMs = 1500) {
  const defaultLabel = button.textContent ?? "Copy Logs";
  let resetTimer: ReturnType<typeof setTimeout> | null = null;

  return () => {
    if (resetTimer !== null) {
      clearTimeout(resetTimer);
    }

    button.textContent = "Copied";
    resetTimer = setTimeout(() => {
      button.textContent = defaultLabel;
      resetTimer = null;
    }, copiedMs);
  };
}
