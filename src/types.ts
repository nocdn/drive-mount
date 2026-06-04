export type CloudProvider = "B2" | "GoogleDrive" | "Seedbox";
export type MountOperation = "mounting" | "unmounting" | "restarting" | null;

export interface BucketMount {
  bucketName: string;
  driveLetter: string;
}

export interface GoogleDriveSettings {
  remotePath: string;
  rootFolderId: string;
}

export interface SeedboxSettings {
  host: string;
  username: string;
  port: number;
  remotePath: string;
  allowUnverifiedCertificate: boolean;
  readOnly: boolean;
}

export interface AppSettings {
  selectedProvider: CloudProvider;
  buckets: BucketMount[];
  googleDrive: GoogleDriveSettings;
  seedbox: SeedboxSettings;
  startAtLogin: boolean;
  startMinimized: boolean;
}

export interface LoadedSettings {
  settings: AppSettings;
  hasSavedCredentials: boolean;
  isGoogleDriveConfigured: boolean;
  isSeedboxConfigured: boolean;
  hasSavedSeedboxPassword: boolean;
}

export interface LogLine {
  level: string;
  message: string;
  timestamp: string;
}

export interface MountEntry {
  label: string;
  target: string;
  provider: CloudProvider;
  status: string;
  pid?: number;
}

export interface MountState {
  mounted: boolean;
  mounts?: MountEntry[];
  errors?: string[];
}
