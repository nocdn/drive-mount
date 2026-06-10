import type { AppSettings, BucketMount } from "./types";

interface MountRelevantSettings {
  buckets: BucketMount[];
  googleDrive: {
    remotePath: string;
    rootFolderId: string;
  };
  oneDrive: {
    remotePath: string;
  };
  seedbox: {
    host: string;
    username: string;
    port: number;
    remotePath: string;
    allowUnverifiedCertificate: boolean;
    readOnly: boolean;
  };
}

function normalizeDriveLetter(value: string): string {
  return value.trim().replace(/:$/, "").toUpperCase();
}

function normalizeBuckets(buckets: BucketMount[], platform: string): BucketMount[] {
  return buckets
    .map((bucket) => ({
      bucketName: bucket.bucketName.trim(),
      driveLetter: platform === "macos" ? "" : normalizeDriveLetter(bucket.driveLetter),
    }))
    .filter((bucket) => bucket.bucketName !== "");
}

export function mountRelevantSettingsSnapshot(settings: AppSettings, platform: string): string {
  const relevant: MountRelevantSettings = {
    buckets: normalizeBuckets(settings.buckets, platform),
    googleDrive: {
      remotePath: settings.googleDrive.remotePath.trim(),
      rootFolderId: settings.googleDrive.rootFolderId.trim(),
    },
    oneDrive: {
      remotePath: settings.oneDrive.remotePath.trim(),
    },
    seedbox: {
      host: settings.seedbox.host.trim(),
      username: settings.seedbox.username.trim(),
      port: settings.seedbox.port,
      remotePath: settings.seedbox.remotePath.trim(),
      allowUnverifiedCertificate: settings.seedbox.allowUnverifiedCertificate,
      readOnly: settings.seedbox.readOnly,
    },
  };

  return JSON.stringify(relevant);
}

export function hasMountRelevantSettingsChanges(
  settings: AppSettings,
  activeSnapshot: string | null,
  platform: string,
): boolean {
  return activeSnapshot !== null && mountRelevantSettingsSnapshot(settings, platform) !== activeSnapshot;
}
