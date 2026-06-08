import { describe, expect, test } from "bun:test";
import { createCopiedButtonFeedback } from "../src/copyButtonFeedback";

function buttonWithLabel(textContent: string): HTMLButtonElement {
  return { textContent } as HTMLButtonElement;
}

function wait(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

describe("createCopiedButtonFeedback", () => {
  test("shows Copied and restores the original label", async () => {
    const button = buttonWithLabel("Copy Logs");
    const showFeedback = createCopiedButtonFeedback(button, 5);

    showFeedback();

    expect(button.textContent).toBe("Copied");
    await wait(15);
    expect(button.textContent).toBe("Copy Logs");
  });

  test("restarts the restore timeout when feedback is shown again", async () => {
    const button = buttonWithLabel("Copy Logs");
    const showFeedback = createCopiedButtonFeedback(button, 25);

    showFeedback();
    await wait(15);
    showFeedback();
    await wait(15);

    expect(button.textContent).toBe("Copied");
    await wait(20);
    expect(button.textContent).toBe("Copy Logs");
  });
});
