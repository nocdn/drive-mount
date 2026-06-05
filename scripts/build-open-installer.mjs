import { execFile, spawn } from "node:child_process";
import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const run = promisify(execFile);

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const bundleRoot = path.join(repoRoot, "src-tauri", "target", "release", "bundle");
const packageJsonPath = path.join(repoRoot, "package.json");
const tauriConfigPath = path.join(repoRoot, "src-tauri", "tauri.conf.json");
const cargoTomlPath = path.join(repoRoot, "src-tauri", "Cargo.toml");

const platformConfig = {
  darwin: {
    bundle: "dmg",
    extension: ".dmg",
    async installedVersion(appMetadata) {
      return installedMacosVersion(appMetadata);
    },
    open(installerPath) {
      return run("open", [installerPath], { cwd: repoRoot });
    },
  },
  win32: {
    bundle: "msi",
    extension: ".msi",
    async installedVersion(appMetadata) {
      return installedWindowsVersion(appMetadata);
    },
    open(installerPath) {
      return run(
        "powershell.exe",
        [
          "-NoProfile",
          "-ExecutionPolicy",
          "Bypass",
          "-Command",
          `Start-Process -FilePath ${powerShellSingleQuoted(installerPath)}`,
        ],
        { cwd: repoRoot },
      );
    },
  },
};

function powerShellSingleQuoted(value) {
  return `'${value.replace(/'/g, "''")}'`;
}

async function readJson(filePath) {
  return JSON.parse(await fs.readFile(filePath, "utf8"));
}

async function writeJson(filePath, value) {
  await fs.writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function parseVersion(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version?.trim() ?? "");
  if (!match) {
    throw new Error(`Expected a plain major.minor.patch version, got "${version}"`);
  }

  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
  };
}

function formatVersion(version) {
  return `${version.major}.${version.minor}.${version.patch}`;
}

function compareVersions(left, right) {
  for (const key of ["major", "minor", "patch"]) {
    if (left[key] !== right[key]) {
      return left[key] - right[key];
    }
  }

  return 0;
}

function bumpPatch(version) {
  return {
    ...version,
    patch: version.patch + 1,
  };
}

function newestVersionFromLines(output) {
  const versions = output
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);

  if (versions.length === 0) {
    return null;
  }

  return versions
    .map((version) => ({ raw: version, parsed: parseVersion(version) }))
    .sort((left, right) => compareVersions(right.parsed, left.parsed))[0].raw;
}

async function readAppMetadata() {
  const tauriConfig = await readJson(tauriConfigPath);
  return {
    productName: tauriConfig.productName,
    identifier: tauriConfig.identifier,
    version: tauriConfig.version,
  };
}

async function installedMacosVersion({ productName, identifier }) {
  const appPath = path.join("/Applications", `${productName}.app`);
  const plistPath = path.join(appPath, "Contents", "Info.plist");

  try {
    const { stdout } = await run("/usr/libexec/PlistBuddy", [
      "-c",
      "Print :CFBundleShortVersionString",
      plistPath,
    ]);
    return stdout.trim() || null;
  } catch {
    // Fall back to Spotlight because the app may not be installed in /Applications.
  }

  try {
    const { stdout } = await run("mdfind", [`kMDItemCFBundleIdentifier == "${identifier}"`]);
    const [foundAppPath] = stdout
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);

    if (!foundAppPath) {
      return null;
    }

    const { stdout: version } = await run("/usr/libexec/PlistBuddy", [
      "-c",
      "Print :CFBundleShortVersionString",
      path.join(foundAppPath, "Contents", "Info.plist"),
    ]);
    return version.trim() || null;
  } catch {
    return null;
  }
}

async function installedWindowsVersion({ productName }) {
  const script = `
$paths = @(
  'HKLM:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*',
  'HKLM:\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*',
  'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*'
)
Get-ItemProperty -Path $paths -ErrorAction SilentlyContinue |
  Where-Object { $_.DisplayName -eq $args[0] -and $_.DisplayVersion } |
  Select-Object -ExpandProperty DisplayVersion
`;

  try {
    const { stdout } = await run("powershell.exe", [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-Command",
      script,
      productName,
    ]);
    return newestVersionFromLines(stdout);
  } catch {
    return null;
  }
}

function replaceCargoPackageVersion(cargoToml, nextVersion) {
  return cargoToml.replace(
    /^version = "([^"]+)"$/m,
    `version = "${nextVersion}"`,
  );
}

async function syncProjectVersion(nextVersion) {
  const tauriConfig = await readJson(tauriConfigPath);
  tauriConfig.version = nextVersion;
  await writeJson(tauriConfigPath, tauriConfig);

  const packageJson = await readJson(packageJsonPath);
  packageJson.version = nextVersion;
  await writeJson(packageJsonPath, packageJson);

  const cargoToml = await fs.readFile(cargoTomlPath, "utf8");
  await fs.writeFile(cargoTomlPath, replaceCargoPackageVersion(cargoToml, nextVersion));
}

async function bumpVersionPastInstalled(currentPlatform) {
  const appMetadata = await readAppMetadata();
  const installedVersion = await currentPlatform.installedVersion(appMetadata);

  if (!installedVersion) {
    console.log("No installed version found. Keeping project version:", appMetadata.version);
    return appMetadata.version;
  }

  const currentVersion = parseVersion(appMetadata.version);
  const minimumNextVersion = bumpPatch(parseVersion(installedVersion));
  const nextVersion =
    compareVersions(currentVersion, minimumNextVersion) >= 0
      ? appMetadata.version
      : formatVersion(minimumNextVersion);

  console.log(`Installed version: ${installedVersion}`);
  if (nextVersion === appMetadata.version) {
    console.log(`Project version ${appMetadata.version} is already newer. Keeping it.`);
    return appMetadata.version;
  }

  console.log(`Bumping project version ${appMetadata.version} -> ${nextVersion}`);
  await syncProjectVersion(nextVersion);
  return nextVersion;
}

async function findLatestInstaller(extension) {
  const matches = [];

  async function walk(dir) {
    const entries = await fs.readdir(dir, { withFileTypes: true });
    for (const entry of entries) {
      const entryPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        await walk(entryPath);
        continue;
      }

      if (path.extname(entry.name).toLowerCase() !== extension) {
        continue;
      }

      const stat = await fs.stat(entryPath);
      matches.push({ path: entryPath, mtimeMs: stat.mtimeMs });
    }
  }

  await walk(bundleRoot);
  matches.sort((left, right) => right.mtimeMs - left.mtimeMs);
  return matches[0]?.path ?? null;
}

async function removeExistingInstallers(extension) {
  async function walk(dir) {
    let entries;
    try {
      entries = await fs.readdir(dir, { withFileTypes: true });
    } catch (error) {
      if (error?.code === "ENOENT") {
        return;
      }

      throw error;
    }

    for (const entry of entries) {
      const entryPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        await walk(entryPath);
        continue;
      }

      if (path.extname(entry.name).toLowerCase() === extension) {
        await fs.rm(entryPath, { force: true });
      }
    }
  }

  await walk(bundleRoot);
}

function runStreaming(command, args, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd,
      stdio: "inherit",
      shell: process.platform === "win32",
    });

    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
        return;
      }

      reject(new Error(`${command} exited with code ${code ?? "unknown"}`));
    });
  });
}

async function main() {
  const current = platformConfig[process.platform];
  if (!current) {
    throw new Error(
      `Unsupported platform ${process.platform}. This helper only supports macOS and Windows.`,
    );
  }

  console.log(`Building ${current.bundle.toUpperCase()} installer for ${process.platform}...`);
  const buildVersion = await bumpVersionPastInstalled(current);
  await removeExistingInstallers(current.extension);
  await runStreaming(
    "bun",
    ["run", `build:installer:${process.platform === "darwin" ? "mac" : "windows"}`],
    repoRoot,
  );

  const installerPath = await findLatestInstaller(current.extension);
  if (!installerPath) {
    throw new Error(
      `Build finished but no ${current.extension} installer was found under ${bundleRoot}.`,
    );
  }

  console.log(`Opening installer: ${installerPath}`);
  console.log(`Installer version: ${buildVersion}`);
  await current.open(installerPath);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
