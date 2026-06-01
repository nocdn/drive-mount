import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const run = promisify(execFile);

const version = "v1.74.1";
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const binDir = path.resolve(scriptDir, "../src-tauri/binaries");

const targets = {
  "aarch64-apple-darwin": {
    archiveName: `rclone-${version}-osx-arm64.zip`,
    binaryName: "rclone-aarch64-apple-darwin",
    executableName: "rclone",
    sha256: "98c04f5f678fe87d435d6f4b1fe204103c5906b151357e631ba0111410691213",
  },
  "x86_64-apple-darwin": {
    archiveName: `rclone-${version}-osx-amd64.zip`,
    binaryName: "rclone-x86_64-apple-darwin",
    executableName: "rclone",
    sha256: "4f10d7845422d8568e187a0f6813f124bca9b657ac7becd8bdf8508fa968a336",
  },
  "x86_64-pc-windows-msvc": {
    archiveName: `rclone-${version}-windows-amd64.zip`,
    binaryName: "rclone-x86_64-pc-windows-msvc.exe",
    executableName: "rclone.exe",
    sha256: "51326acc0d9cf60234aa5787d8da66a621430aa373542a6b35bad8a4a26ca43e",
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

async function extractZip(zipPath, destDir) {
  await fs.mkdir(destDir, { recursive: true });

  if (process.platform === "win32") {
    await run("powershell.exe", [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-Command",
      "Expand-Archive -LiteralPath $args[0] -DestinationPath $args[1] -Force",
      zipPath,
      destDir,
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

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
