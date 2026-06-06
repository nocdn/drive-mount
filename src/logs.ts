import type { LogLine } from "./types";

const MAX_RENDERED_LOG_LINES = 1000;

export function formatLogTimestamp(date = new Date()): string {
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  const seconds = String(date.getSeconds()).padStart(2, "0");
  return `${hours}:${minutes}:${seconds}`;
}

export interface LogController {
  appendLog(line: LogLine): void;
  clearRenderedLogs(): void;
  flushPendingLogLines(): void;
}

export function createLogController(logsEl: HTMLPreElement): LogController {
  const pendingLogLines: LogLine[] = [];
  const renderedLogNodes: HTMLSpanElement[] = [];
  let logFlushScheduled = false;

  function createLogLineNode(line: LogLine): HTMLSpanElement {
    const node = document.createElement("span");
    node.className = "log-line";

    const timestamp = document.createElement("span");
    timestamp.className = "log-timestamp";
    timestamp.textContent = `[${line.timestamp}] `;

    const level = document.createElement("span");
    level.className = "log-level";
    level.textContent = `[${line.level}]`;

    node.append(timestamp, level, document.createTextNode(` ${line.message}\n`));
    return node;
  }

  function flushPendingLogLines() {
    logFlushScheduled = false;
    if (pendingLogLines.length === 0) {
      return;
    }

    const shouldStickToBottom = logsEl.scrollTop + logsEl.clientHeight >= logsEl.scrollHeight - 8;
    const lines = pendingLogLines.splice(0, pendingLogLines.length);
    const fragment = document.createDocumentFragment();

    for (const line of lines) {
      const node = createLogLineNode(line);
      renderedLogNodes.push(node);
      fragment.appendChild(node);
    }

    logsEl.appendChild(fragment);

    while (renderedLogNodes.length > MAX_RENDERED_LOG_LINES) {
      renderedLogNodes.shift()?.remove();
    }

    if (shouldStickToBottom) {
      logsEl.scrollTop = logsEl.scrollHeight;
    }
  }

  function appendLog(line: LogLine) {
    pendingLogLines.push(line);
    if (!logFlushScheduled) {
      logFlushScheduled = true;
      requestAnimationFrame(flushPendingLogLines);
    }
  }

  function clearRenderedLogs() {
    pendingLogLines.length = 0;
    renderedLogNodes.length = 0;
    logsEl.textContent = "";
  }

  return {
    appendLog,
    clearRenderedLogs,
    flushPendingLogLines,
  };
}
