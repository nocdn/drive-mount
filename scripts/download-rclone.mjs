import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const run = promisify(execFile);

export const version = "v1.75.1";
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const binDir = path.resolve(scriptDir, "../src-tauri/binaries");

export const targets = {
  "aarch64-apple-darwin": {
    archiveName: `rclone-${version}-osx-arm64.zip`,
    binaryName: "rclone-aarch64-apple-darwin",
    executableName: "rclone",
    sha256: "c61d7a371c62bcbbe882c3423aa4b8bf63485c248dd0f692997b8f0c3f6d0c6f",
  },
  "x86_64-apple-darwin": {
    archiveName: `rclone-${version}-osx-amd64.zip`,
    binaryName: "rclone-x86_64-apple-darwin",
    executableName: "rclone",
    sha256: "29253d0288b8fbbac46baad6e5f6add6cb01d462c79f10805bbd4631c4cdf82c",
  },
  "x86_64-pc-windows-msvc": {
    archiveName: `rclone-${version}-windows-amd64.zip`,
    binaryName: "rclone-x86_64-pc-windows-msvc.exe",
    executableName: "rclone.exe",
    sha256: "200eb602c126d82aa38b51e0f6b9ae837473ff99b51278d3f6f837574c494d6e",
  },
};

function normalizeTargetTriple() {
  const tauriTarget = process.env.TAURI_ENV_TARGET_TRIPLE;
  if (tauriTarget && targets[tauriTarget]) {
    return tauriTarget;
  }

  const tauriPlatform = process.env.TAURI_ENV_PLATFORM;
  const tauriArch = process.env.TAURI_ENV_ARCH;
  const platform = tauriPlatform ?? process.platform;
  const arch = tauriArch ?? process.arch;

  if ((platform === "darwin" || platform === "macos") && (arch === "arm64" || arch === "aarch64")) {
    return "aarch64-apple-darwin";
  }

  if ((platform === "darwin" || platform === "macos") && (arch === "x64" || arch === "x86_64")) {
    return "x86_64-apple-darwin";
  }

  if ((platform === "win32" || platform === "windows") && (arch === "x64" || arch === "x86_64")) {
    return "x86_64-pc-windows-msvc";
  }

  throw new Error(`Unsupported rclone sidecar target: platform=${platform} arch=${arch}`);
}

async function exists(filePath) {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

async function existingBinaryMatches(dest, targetTriple) {
  if (!(await exists(dest))) {
    return false;
  }

  const canExecute =
    (process.platform === "darwin" && targetTriple.endsWith("apple-darwin")) ||
    (process.platform === "win32" && targetTriple.endsWith("windows-msvc"));

  if (!canExecute) {
    console.log(`rclone already present at ${dest}`);
    return true;
  }

  try {
    const { stdout } = await run(dest, ["version"]);
    if (stdout.split(/\r?\n/, 1)[0] === `rclone ${version}`) {
      console.log(`rclone already present at ${dest}`);
      return true;
    }
  } catch {
    return false;
  }

  return false;
}

function powerShellSingleQuoted(value) {
  return `'${value.replace(/'/g, "''")}'`;
}

async function extractZip(zipPath, destDir) {
  await fs.mkdir(destDir, { recursive: true });

  if (process.platform === "win32") {
    const command = [
      "Expand-Archive",
      "-LiteralPath",
      powerShellSingleQuoted(zipPath),
      "-DestinationPath",
      powerShellSingleQuoted(destDir),
      "-Force",
    ].join(" ");

    await run("powershell.exe", [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-Command",
      command,
    ]);
    return;
  }

  if (process.platform === "darwin") {
    await run("ditto", ["-x", "-k", zipPath, destDir]);
    return;
  }

  await run("unzip", ["-q", zipPath, "-d", destDir]);
}

async function findExtractedBinary(root, executableName) {
  const entries = await fs.readdir(root, { withFileTypes: true });
  for (const entry of entries) {
    const entryPath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      const found = await findExtractedBinary(entryPath, executableName);
      if (found) {
        return found;
      }
    } else if (entry.name === executableName) {
      return entryPath;
    }
  }
  return null;
}

async function main() {
  const targetTriple = normalizeTargetTriple();
  const target = targets[targetTriple];
  const dest = path.join(binDir, target.binaryName);

  await fs.mkdir(binDir, { recursive: true });
  if (await existingBinaryMatches(dest, targetTriple)) {
    return;
  }

  const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "cloud-drive-mount-rclone-"));
  try {
    const url = `https://github.com/rclone/rclone/releases/download/${version}/${target.archiveName}`;
    const zipPath = path.join(tmpDir, "rclone.zip");
    const unzipDir = path.join(tmpDir, "unzipped");

    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(`Failed to download ${url}: ${response.status} ${response.statusText}`);
    }

    const archive = Buffer.from(await response.arrayBuffer());
    const actualSha256 = createHash("sha256").update(archive).digest("hex");
    if (actualSha256 !== target.sha256) {
      throw new Error(`Unexpected SHA-256 for ${target.archiveName}: ${actualSha256}`);
    }

    await fs.writeFile(zipPath, archive);
    await extractZip(zipPath, unzipDir);

    const found = await findExtractedBinary(unzipDir, target.executableName);
    if (!found) {
      throw new Error(`Downloaded archive did not contain ${target.executableName}`);
    }

    await fs.copyFile(found, dest);
    await fs.chmod(dest, 0o755);
    console.log(`Downloaded rclone to ${dest}`);
  } finally {
    await fs.rm(tmpDir, { recursive: true, force: true });
  }
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exit(1);
  });
}
