import type { LogLine } from "./types";

const MAX_RENDERED_LOG_LINES = 1000;

export interface LogController {
  appendLog(line: LogLine): void;
  clearRenderedLogs(): void;
  flushPendingLogLines(): void;
}

export function createLogController(logsEl: HTMLPreElement): LogController {
  const pendingLogLines: string[] = [];
  const renderedLogNodes: Text[] = [];
  let logFlushScheduled = false;

  function flushPendingLogLines() {
    logFlushScheduled = false;
    if (pendingLogLines.length === 0) {
      return;
    }

    const shouldStickToBottom = logsEl.scrollTop + logsEl.clientHeight >= logsEl.scrollHeight - 8;
    const lines = pendingLogLines.splice(0, pendingLogLines.length);
    const fragment = document.createDocumentFragment();

    for (const line of lines) {
      const node = document.createTextNode(line);
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
    pendingLogLines.push(`[${line.timestamp}] [${line.level}] ${line.message}\n`);
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
