# OneDrive Mounting — Implementation Plan

> Status: **Plan / not yet implemented.** This document is a complete, self-contained
> brief for an agent (or developer) to add Microsoft OneDrive as a mountable provider in
> this app, with feature parity to the existing Google Drive provider.
>
> Audience: an agent who has **not** seen this codebase before. Read the
> [Codebase orientation](#1-codebase-orientation) section first, then follow
> [The implementation](#5-the-implementation-file-by-file) step by step.
>
> Last researched: **2026-06-10**. rclone OneDrive docs source last updated 2026-03-17.
> Verify the linked docs again before starting (see [Links to read first](#9-links-to-read-yourself-before-starting)).

---

## 0. TL;DR

- **Feasibility: high.** OneDrive slots into the existing provider model almost exactly
  like Google Drive. ~80% of the work is mechanical copy-paste of the Google Drive
  provider across the same layers.
- **The one hard part:** OneDrive's rclone OAuth setup is **not** a single self-completing
  command like Google Drive. After the browser OAuth, rclone must call the Microsoft Graph
  API to enumerate drives and then have `drive_id` + `drive_type` selected. There are two
  ways to handle this (see [Section 4](#4-the-one-hard-part-the-onedrive-oauth-flow)).
- **Recommended first cut:** **Strategy A** (single blocking `rclone config create` that
  lets rclone take defaults). It is nearly identical to today's Google Drive code and works
  for the common case (personal OneDrive, single drive). Fall back to **Strategy B**
  (drive the `--non-interactive` JSON state machine) only if Strategy A proves unreliable
  during verification, or to support multi-drive/business accounts.
- **Scope:** personal OneDrive, Microsoft Cloud Global region. macOS mounts at
  `~/Drives/onedrive`; Windows mounts at drive letter `O:`.

---

## 1. Codebase orientation

This is a Tauri v2 app: a TypeScript/Vite frontend (`src/`) talks to a Rust backend
(`src-tauri/src/`) over Tauri IPC (`invoke`). Mounting is done by shelling out to a
bundled `rclone` sidecar binary. The user runs the **Tauri app on both macOS and Windows**
(see `AGENTS.md`).

### How a provider works end-to-end (using Google Drive as the reference)

```diagram
╭─────────────╮  invoke("configure_google_drive_cmd")  ╭────────────────────────────╮
│  src/main.ts│ ─────────────────────────────────────▶ │ commands.rs                │
│  (UI panel) │                                          │  configure_google_drive_cmd│
╰─────────────╯                                          ╰─────────────┬──────────────╯
       ▲                                                                │
       │ MountState / logs (events)                                     ▼
       │                                              ╭──────────────────────────────────╮
       │                                              │ rclone/mod.rs                      │
       │                                              │  RcloneController::configure_*     │
       │                                              │   → rclone config create ... (OAuth│
       │                                              │     opens browser)                 │
       │                                              │   → save_*_config() to keyring     │
       │                                              ╰──────────────────────────────────╯
       │  Save & Mount All                                             │
       ▼                                                                ▼
  build_*_spec() → build_mount_command_args() → `rclone mount remote: <target>`
```

### Files you will touch (all relative to repo root)

| Layer | File | What lives there |
|-------|------|------------------|
| Provider enum + settings structs | `src-tauri/src/models.rs` | `CloudProvider`, `GoogleDriveSettings`, `GDRIVE_REMOTE`, Windows drive-letter constants, `MountRequest`, `LoadedCredentials` |
| Mount/config logic | `src-tauri/src/rclone/mod.rs` | `RcloneController` methods (`configure_google_drive`, `build_google_drive_spec`, `is_google_drive_configured`, `ensure_google_drive_config`, `build_all_mount_specs`, `configured_mount_targets`, `has_complete_google_drive_config`) |
| rclone.conf editing | `src-tauri/src/rclone/config.rs` | `upsert_config_section`, `read_config_section_lines`, `remove_config_section`, `has_config_section` (generic — reuse as-is) |
| Secret persistence | `src-tauri/src/credentials.rs` | keyring bundle: `save_google_drive_config`, `load_google_drive_config`, `delete_google_drive_config`, `has_saved_google_drive_config` |
| Mount targets / paths | `src-tauri/src/paths.rs` | `GOOGLE_DRIVE_MOUNT_NAME`, `default_google_drive_mount_path`, `normalize_google_drive_path` |
| Platform mount target | `src-tauri/src/rclone/platform/macos.rs`, `.../windows.rs`, `.../mod.rs` | `google_drive_mount_target`, drive-letter reservation/validation |
| Tauri commands | `src-tauri/src/commands.rs` | `configure_google_drive_cmd`, `disconnect_google_drive_cmd`, `test_google_drive_connection_cmd`, `is_google_drive_configured_cmd`, `saved_mount_request`, `load_credentials_cmd` |
| Command registration | `src-tauri/src/lib.rs` | `tauri::generate_handler![ ... ]` |
| Frontend types | `src/types.ts` | `CloudProvider`, `GoogleDriveSettings`, `LoadedCredentials`, etc. |
| Frontend logic | `src/main.ts` | provider panel toggling, gdrive connect/test handlers, settings read/write |
| Frontend markup | `index.html` | `#provider` `<select>`, `#gdrive-panel` |
| Validation | `src/validation.ts` | reserved drive letters, per-provider field validation |
| Tests | `tests/*.test.ts` (frontend), and `#[cfg(test)]` modules inside each `.rs` file | serde contracts, mount-target shape, settings round-trip |

> **Important:** there is no single grep that finds "everywhere Google Drive is wired in."
> Use `rg -i "google_drive|gdrive|GoogleDrive"` across `src-tauri/src` and `src` to build
> the exhaustive list, then mirror each hit for OneDrive.

---

## 2. How Google Drive is configured today (the template to copy)

In `src-tauri/src/rclone/mod.rs`, `RcloneController::configure_google_drive` runs a **single
blocking** rclone command (it does **not** use `--non-interactive`):

```text
rclone config create gdrive drive \
  scope drive \
  config_is_local true \
  --no-output \
  [root_folder_id <ID>]          # only if user supplied one
  --config <app rclone.conf path>
```

Key facts about this flow:
- `config_is_local true` makes rclone spin up a local auth web server and open the system
  browser. The command **blocks** until the user finishes the browser sign-in.
- Because `--non-interactive` is **not** passed, rclone auto-takes the **default** answer for
  any remaining config questions. For Google Drive the only post-OAuth question is
  `config_team_drive` (defaults to "no"), so the command completes on its own.
- On success, the `[gdrive]` section (with the `token = {...}` blob) is written to the app's
  rclone.conf, then `read_config_section_lines` reads it back and `save_google_drive_config`
  stores those lines in the OS keyring (so the token survives even if the conf file is
  deleted). `ensure_google_drive_config` re-materialises the conf section from the keyring
  before any mount/test.

The mount itself (`build_google_drive_spec` → `build_mount_command_args`) runs:

```text
rclone mount gdrive:<optional path> <target> \
  --config <conf> --cache-dir <cache> --vfs-cache-mode full \
  <volume-name args> --links --log-level NOTICE <platform extra args>
```

OneDrive will mirror all of this. The **only** behavioural difference is the config step.

---

## 3. Research findings — rclone + OneDrive (current as of 2026-06)

Everything below was gathered from rclone's official docs, the rclone forum, and rclone's
GitHub. Treat it as authoritative-but-verify; Microsoft changes OneDrive behaviour
periodically.

### 3.1 What a finished OneDrive remote looks like

A working personal OneDrive section in rclone.conf is just:

```ini
[onedrive]
type = onedrive
token = {"access_token":"...","token_type":"Bearer","refresh_token":"...","expiry":"2026-..."}
drive_id = BF77630123456789
drive_type = personal
# region = global    # only needed if not the default global cloud
```

For OneDrive **Business** / SharePoint, `drive_type = business` (or `documentLibrary`) and
`drive_id` is a long `b!...` string. This plan targets **personal** first.

### 3.2 rclone OneDrive config options that matter

- `region` — national cloud. Default `global` (Microsoft Cloud Global). Other values:
  `us` (US Gov), `de` (deprecated), `cn` (Vnet/China). **Use `global`.**
- `drive_id` — ID of the drive to mount. **Discovered during config**, not user-entered for
  the common case.
- `drive_type` — `personal` | `business` | `documentLibrary`. **Discovered during config.**
- `root_folder_id` — optional; mount a specific folder by ID (analogous to Google Drive's
  `root_folder_id`). Usually unnecessary.
- `client_id` / `client_secret` — leave blank to use rclone's shared default app
  registration (same as Google Drive). Only set these if the user hits throttling and wants
  their own Azure app. **Leave blank for v1.**
- `tenant` — only for client-credential/business single-tenant flows. **Ignore for v1.**

### 3.3 Mount tuning / behavioural notes

- OneDrive **rate-limits more aggressively** than Google Drive. If listings/transfers get
  throttled, raise `--checkers` / lower concurrency, or eventually add `--onedrive-delta`
  (`delta = true`) for efficient recursive listing (recommended only when mounting at/near
  the drive root and using `vfs/refresh`).
- `--vfs-cache-mode full` is appropriate (same as Google Drive in this app).
- OneDrive **Business** creates a new file version every time rclone sets mod-time after
  upload; `--onedrive-no-versions` mitigates quota bloat (do **not** use on personal — it
  can't delete versions). Not needed for personal v1.
- Default hash is QuickXorHash for all OneDrive types (since rclone 1.62). No action needed.

### 3.4 Hard limitations / gotchas (call these out to the user)

1. **Shared folders need owner auth.** rclone must authenticate **as the owner** of a
   OneDrive folder. You cannot rclone a folder merely shared *with* you without signing in
   as the sharer. Fine for the user's own account.
2. **Business "default drive" can be wrong.** On business accounts the default `config_driveid`
   can resolve to `PreservationHoldLibrary` instead of `Documents`. This is the main reason
   Strategy B (explicit drive selection) exists. Not an issue for single-drive personal
   accounts.
3. **Refresh token expires after 90 days of non-use.** Recovery is
   `rclone config reconnect onedrive:` (re-runs OAuth). Consider exposing a "Reconnect"
   affordance later; for v1 the existing Disconnect+Connect flow covers it.
4. **`access_denied (AADSTS65005)`** means the org hasn't enabled the rclone app — a
   business/tenant admin issue, not something the app can fix. Surface the error text.
5. **OneNote files** are hidden by default in listings (operations don't work on them).
   `--onedrive-expose-onenote-files` shows them. Leave default for v1.

---

## 4. The one hard part: the OneDrive OAuth flow

Unlike Google Drive, OneDrive's rclone config does a **Microsoft Graph call after OAuth** to
enumerate drives, then needs `config_type` and `config_driveid` answered. The full state
machine (observed from rclone `-vv` debug logs) is:

```text
""                              → *oauth,choose_type,,
*oauth,choose_type,,            → *oauth-confirm,choose_type,,
*oauth-confirm,choose_type,,    → (config_is_local) → browser OAuth runs here
... browser sign-in ...         → *oauth-done,choose_type,,
*oauth-done,choose_type,,       → choose_type
choose_type                     → choose_type_done   [Option: config_type, default "onedrive"]
choose_type_done (result onedrive) → onedrive         [queries Graph /me/drives]
onedrive                        → driveid_final       [Option: config_driveid, Examples = found drives]
driveid_final (result <drive_id>)  → driveid_final_end [confirms "Drive OK?"]
driveid_final_end (result true) → ""  (done)
```

### Strategy A — single blocking command (RECOMMENDED for v1)

Mirror Google Drive exactly. Run, **without** `--non-interactive`:

```text
rclone config create onedrive onedrive \
  region global \
  config_is_local true \
  --no-output \
  --config <app rclone.conf path>
```

Rationale: because `--non-interactive` is omitted, rclone auto-takes the **default** answer
for `config_type` (`onedrive`) and `config_driveid` (the first/default drive). For a personal
account with a single drive this produces the correct `drive_id` + `drive_type`
automatically, exactly like the Google Drive `config_team_drive` default.

- **Pros:** ~5 lines, identical shape to existing `configure_google_drive`. Reuses
  `read_config_section_lines` + `save_google_drive_config` machinery verbatim.
- **Cons:** picks the **default** drive. On business/multi-drive accounts that may be the
  wrong drive (the `PreservationHoldLibrary` problem). Acceptable for personal v1.
- **⚠️ MUST VERIFY during implementation:** confirm that the blocking `config create` for
  OneDrive actually auto-completes and writes a `[onedrive]` section containing `drive_id`
  and `drive_type` (not just `token`). If the section is missing `drive_id`/`drive_type`,
  Strategy A is insufficient → use Strategy B. Verify by running the command by hand against
  a real personal account and inspecting the conf (see [Section 8](#8-verification--testing)).

### Strategy B — drive the `--non-interactive` JSON state machine (fallback / business)

rclone's documented automation protocol (see
`https://rclone.org/commands/rclone_config_create/`):

1. `rclone config create onedrive onedrive region global config_is_local true --non-interactive --config <conf>`
   → returns a JSON blob `{ "State": "...", "Option": {...} }` (or runs the browser for the
   oauth-do step, then returns the next question).
2. Loop: read `State` + `Option`, decide the `--result`, then
   `rclone config update onedrive --continue --state "<State>" --result "<Result>" --non-interactive --config <conf>`.
3. Repeat until rclone returns a result whose `State` is the empty string (done).

The deterministic post-OAuth sequence for "personal, take first drive" (confirmed from a
forum automation example) is:

```text
--state "*oauth-confirm,choose_type,," --result "false"
--state "choose_type_done"             --result "onedrive"   # → returns config_driveid options
--continue --state "driveid_final"     --result "<drive_id from Option.Examples[0].Value>"
--continue --state "driveid_final_end" --result "true"
```

- **Pros:** robust; lets you enumerate drives from `Option.Examples` and (optionally) let the
  user pick which drive; correctly handles business/multi-drive.
- **Cons:** ~50–80 lines of Rust: spawn rclone capturing stdout, parse JSON (serde_json),
  loop with state/result, surface drive choices to the UI if you want selection.
- rclone ships a readable reference implementation of this protocol at `bin/config.py` in the
  rclone source — read it if implementing Strategy B.

**Decision:** Implement Strategy A first. Keep this section as the spec for Strategy B and
add a `// TODO(onedrive-business)` note in `configure_onedrive` pointing here.

---

## 5. The implementation, file by file

Follow Google Drive as the template for every change. Names below assume the symbol
`OneDrive` / `onedrive` / `ONEDRIVE`.

### 5.1 `src-tauri/src/models.rs`

- Add enum variant:
  ```rust
  #[serde(rename = "OneDrive")]
  OneDrive,
  ```
  to `CloudProvider` (keep `#[serde(rename_all = "PascalCase")]` behaviour; the explicit
  rename keeps the frontend contract string `"OneDrive"`).
- Add constant `pub const ONEDRIVE_REMOTE: &str = "onedrive";`.
- Add `#[cfg(windows)] pub const ONEDRIVE_WINDOWS_DRIVE: &str = "O";`.
- Add struct mirroring `GoogleDriveSettings`:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
  #[serde(rename_all = "camelCase")]
  pub struct OneDriveSettings {
      #[serde(default)]
      pub remote_path: String,
      // optional, future: drive_id/drive_type if you implement Strategy B selection
  }
  impl OneDriveSettings {
      pub fn normalized(&self) -> Self {
          let mut s = self.clone();
          s.remote_path = crate::paths::normalize_remote_path(&s.remote_path);
          s
      }
  }
  ```
- Add `pub one_drive: OneDriveSettings` to `AppSettings` (with `#[serde(default)]`) and to
  `MountRequest`. Update `AppSettings::Default` and `AppSettings::normalized`.
- Add `pub is_one_drive_configured: bool` to `LoadedCredentials`.
- Update the `#[cfg(test)]` serde tests in this file: add OneDrive assertions to
  `cloud_provider_serializes_to_frontend_contract_values`,
  `loaded_credentials_serializes_with_camel_case_fields`, and the `MountRequest`/`AppSettings`
  deserialization tests.

### 5.2 `src-tauri/src/paths.rs`

- Add `pub const ONEDRIVE_MOUNT_NAME: &str = "onedrive";`.
- Add `#[cfg(target_os = "macos")] pub fn default_one_drive_mount_path()` returning
  `drives_dir().join(ONEDRIVE_MOUNT_NAME)` (mirror `default_google_drive_mount_path`).
  > Per `AGENTS.md`, macOS mounts must live directly under `~/Drives/` with an exact folder
  > name. Use exactly `onedrive`.

### 5.3 `src-tauri/src/credentials.rs`

- Add `one_drive_config: Option<Vec<String>>` to `SecureCredentials` and include it in
  `is_empty()`.
- Add `save_one_drive_config`, `load_one_drive_config`, `delete_one_drive_config`,
  `has_saved_one_drive_config` (copy the `*_google_drive_config` fns exactly).
- Extend the `secure_credentials_*` unit tests to cover the new field.

### 5.4 `src-tauri/src/rclone/platform/{macos,windows,mod}.rs`

- `mod.rs` (the non-macos/non-windows stub) and both real impls: add
  `pub fn one_drive_mount_target(_settings: &OneDriveSettings) -> String`.
  - macOS: `default_one_drive_mount_path()`.
  - Windows: `format!("{ONEDRIVE_WINDOWS_DRIVE}:")`.
- Windows `validate_drive_letter` / reservation logic: reserve `O:` for OneDrive (currently
  `G` and `S` are reserved for Google Drive and Seedbox — add `O`). Update the matching error
  message and any `#[cfg(test)]` for reserved letters.
- Update the macOS test `google_drive_and_seedbox_targets_use_fixed_service_paths` (or add a
  sibling) to assert `one_drive_mount_target(...).ends_with("/Drives/onedrive")`.

### 5.5 `src-tauri/src/rclone/mod.rs` (the core)

Add OneDrive equivalents of every Google Drive helper. Concretely:

- `configure_one_drive(&self, app, one_drive: &OneDriveSettings)` — **Strategy A**. Copy
  `configure_google_drive` but build args:
  ```text
  config create onedrive onedrive region global config_is_local true --no-output
  ```
  (no `scope`; no `root_folder_id` for v1). Keep the same flow: remove existing section,
  run `run_rclone_blocking(... "OneDrive authorization")`, verify the section exists, then
  `read_config_section_lines(&config_path, ONEDRIVE_REMOTE)` → `save_one_drive_config(...)`.
  After writing, **assert the section contains `drive_id` and `drive_type`** and log an error
  if not (this is the Strategy-A verification guard; see Section 4).
- `disconnect_one_drive(...)` — unmount target, `remove_config_section(ONEDRIVE_REMOTE)`,
  `delete_one_drive_config()`.
- `test_one_drive_connection(...)` — `ensure_one_drive_config()`, then
  `rclone lsd onedrive:<path> --config <conf>` via `run_rclone_blocking`.
- `is_one_drive_configured(&self)` and free fn `is_one_drive_configured()`:
  `has_saved_one_drive_config()` in non-test, `has_config_section(ONEDRIVE_REMOTE)` in test
  (copy the existing `cfg`-split pattern).
- `ensure_one_drive_config()` — re-materialise conf section from keyring (copy
  `ensure_google_drive_config`).
- `build_one_drive_remote_path(...)` → `onedrive:` or `onedrive:<path>`.
- `build_one_drive_volume_name(...)` → `ONEDRIVE_MOUNT_NAME`.
- `build_one_drive_spec(...)` → `MountSpec { label: "OneDrive", provider: CloudProvider::OneDrive,
  vfs_cache_mode: "full", read_only: false, extra_args: Vec::new(), ... }`. (Optionally add
  `extra_args` for throttling later — leave empty for v1.)
- `has_complete_one_drive_config()` (near `has_complete_google_drive_config` ~line 1087).
- Wire into `build_all_mount_specs`: `if is_one_drive_configured() { specs.push(build_one_drive_spec(one_drive)?); }`.
  Update the function signature to take `one_drive: &OneDriveSettings` and update its caller in
  `mount_all` flow. Also update the "Nothing to mount" error string to mention OneDrive.
- Wire into `configured_mount_targets`: push `one_drive_mount_target` when configured.
- Update the top-of-file `use` imports (the `credentials::{...}`, `models::{...}`,
  `paths::{...}` lists) to include the new symbols.

### 5.6 `src-tauri/src/commands.rs`

- New commands (copy the gdrive ones 1:1):
  `is_one_drive_configured_cmd`, `configure_one_drive_cmd`, `disconnect_one_drive_cmd`,
  `test_one_drive_connection_cmd`.
- `load_credentials_cmd`: compute and return `is_one_drive_configured`.
- `mount_all`: pass `one_drive: request.one_drive.normalized()` through.
- `saved_mount_request`: add `has_one_drive = has_complete_one_drive_config();`, include it in
  the early-return guard, and set `one_drive: settings.one_drive`.
- Update the test `saved_mount_request_*` (around line 472–490) to cover OneDrive.

### 5.7 `src-tauri/src/lib.rs`

- Add the four new commands to `tauri::generate_handler![ ... ]` (next to the
  `*_google_drive_cmd` entries around lines 192–195).

### 5.8 `src/types.ts`

- `CloudProvider`: add `"OneDrive"`.
- Add `export interface OneDriveSettings { remotePath: string; }`.
- `AppSettings`: add `oneDrive: OneDriveSettings;`.
- `LoadedCredentials`: add `isOneDriveConfigured: boolean;`.

### 5.9 `index.html`

- Add `<option value="OneDrive">OneDrive</option>` to `#provider`.
- Add a `#onedrive-panel` block mirroring `#gdrive-panel`: a remote-path input
  (`#onedrive-remote-path`), help text (`#onedrive-help`), and buttons
  `#btn-connect-onedrive` + `#btn-test-onedrive`.

### 5.10 `src/main.ts`

- Grab the new DOM elements.
- `updateProviderPanels`: toggle `#onedrive-panel` on `provider === "OneDrive"`.
- Add `readOneDriveSettings()`, `refreshOneDriveConnectionUi()` (mirror the gdrive ones,
  including the macOS-vs-Windows help text: `~/Drives/onedrive` vs `O:`).
- Add `#btn-connect-onedrive` / `#btn-test-onedrive` click handlers calling
  `configure_one_drive_cmd` / `disconnect_one_drive_cmd` / `test_one_drive_connection_cmd`.
- Track `oneDriveConnected` state; set from `loaded.isOneDriveConfigured`.
- Persist/restore `settings.oneDrive` in load/save (mirror `googleDrive`).
- Include `oneDrive` in the object sent to `mount_all` / save (mirror `googleDrive`).

### 5.11 `src/validation.ts`

- Reserved drive letters: add `"O"` alongside `"G"` and `"S"` in the
  `validateB2ForMount` drive-letter reserved set.
- If you add any required OneDrive fields later, add a `validateOneDriveForMount`; for v1
  there are no required text fields (path is optional), so this may be unnecessary.

---

## 6. Config artifact summary

After a successful connect, the app's rclone.conf (path from `rclone_config_path()`) should
contain:

```ini
[onedrive]
type = onedrive
token = {...}
drive_id = <id>
drive_type = personal
```

…and the same lines should be mirrored into the OS keyring via `save_one_drive_config` so the
config survives conf-file deletion (matching Google Drive behaviour). `ensure_one_drive_config`
restores them before mount/test.

---

## 7. Mount command (what will actually run)

```text
rclone mount onedrive:<optional path> <target> \
  --config <conf> --cache-dir <cache> --vfs-cache-mode full \
  <volume-name args> --links --log-level NOTICE <platform extra args>
```

- macOS `<target>`: `~/Drives/onedrive`
- Windows `<target>`: `O:`

If throttling shows up in testing, consider adding to `build_one_drive_spec.extra_args`:
`--tpslimit 10` (or `--onedrive-delta` / `delta = true` in the conf when mounting at root).
Leave empty for v1 unless verification shows a problem.

---

## 8. Verification & testing

Per `AGENTS.md`: when you add a feature, **add/adjust tests and run them; all tests must pass.
Then restart the dev app** (full kill + relaunch for backend/Rust changes; Vite hot-reload is
fine for small UI-only tweaks).

### 8.1 Manual rclone smoke test FIRST (decides Strategy A vs B)

Before writing app code, validate the auth flow by hand with the bundled rclone against a real
personal OneDrive account:

```bash
# find the bundled binary under src-tauri/binaries or use a system rclone of the same version (v1.74.1)
rclone config create onedrive onedrive region global config_is_local true --no-output \
  --config /tmp/od-test.conf
# complete the browser sign-in, then:
cat /tmp/od-test.conf      # MUST contain drive_id AND drive_type, not just token
rclone lsd onedrive: --config /tmp/od-test.conf
```

- If `drive_id` + `drive_type` are present and `lsd` lists folders → **Strategy A works**,
  proceed as planned.
- If `drive_id`/`drive_type` are missing or `lsd` fails with "unable to get drive_id" →
  implement **Strategy B** (Section 4).

### 8.2 Automated tests to add

- **Rust** (`cargo test` from `src-tauri`, or the project's configured runner):
  - `models.rs`: OneDrive serde contract (`"OneDrive"`), `AppSettings`/`MountRequest`
    round-trips include `oneDrive`, `LoadedCredentials` camelCase includes
    `isOneDriveConfigured`.
  - `credentials.rs`: `SecureCredentials` empty-detection + partial-JSON deserialize with
    `one_drive_config`.
  - `rclone/config.rs`: existing generic tests already cover section upsert/read/remove; add an
    `[onedrive]` case if you want belt-and-suspenders.
  - `platform/macos.rs`: `one_drive_mount_target` ends with `/Drives/onedrive`.
  - `commands.rs`: `saved_mount_request` includes OneDrive when configured.
- **Frontend** (`bun run test:frontend` → `bun test tests`):
  - If you extend `mountSettings.ts` / `validation.ts` for OneDrive, mirror the existing tests
    in `tests/mountSettings.test.ts` (and add reserved-letter `O` coverage if you touch
    `validation.ts`).

### 8.3 End-to-end (manual, both platforms)

1. Launch dev app, pick OneDrive, click **Connect OneDrive**, complete browser sign-in.
2. Confirm UI flips to "connected"; **Test Connection** succeeds.
3. **Save and Mount All** → OneDrive appears at `~/Drives/onedrive` (macOS) / `O:` (Windows).
4. Read a file, write a small file, confirm it round-trips.
5. **Disconnect OneDrive** removes the mount and the conf section.
6. Restart app → if config is complete, auto-mount restores OneDrive.

---

## 9. Links to read yourself before starting

Re-fetch these (Microsoft/rclone change over time). The rclone OneDrive doc is the single most
important one.

**rclone official docs**
- OneDrive backend (config, options, limitations, troubleshooting):
  `https://rclone.org/onedrive/`
- `rclone config create` (the `--non-interactive` State/Result protocol — needed for Strategy B):
  `https://rclone.org/commands/rclone_config_create/`
- Remote/headless setup (`rclone authorize`, `config_token`):
  `https://rclone.org/remote_setup/`
- `rclone mount` flags & VFS caching:
  `https://rclone.org/commands/rclone_mount/`
- Global flags / docs index:
  `https://rclone.org/docs/`

**rclone source (read for Strategy B)**
- Reference non-interactive config driver: `bin/config.py` in `https://github.com/rclone/rclone`
- Config command (state/result handling): `https://github.com/rclone/rclone/blob/master/cmd/config/config.go`
- OneDrive backend source (drive discovery, auth_url/token_url hints):
  `https://github.com/rclone/rclone/blob/master/backend/onedrive/onedrive.go`

**Forum / real-world automation references**
- Non-interactive OneDrive drive selection via `--state/--result/--continue` (exact sequence,
  business multi-drive workaround):
  `https://forum.rclone.org/t/rclone-defaults-to-connecting-to-my-onedrive-preservationholdlibrary-folder-instead-of-documents/46611`
- Walkthrough incl. `rclone mount` of OneDrive on Windows/Linux (2025):
  `https://blog.ligos.net/2025-05-30/Connecting-To-OneDrive-With-RClone.html`
- Authorize without interactive session (token plumbing):
  `https://forum.rclone.org/t/authorize-remote-without-interactive-session/40512`

**Microsoft (only if doing custom client_id / business)**
- Graph permissions reference: `https://learn.microsoft.com/en-us/graph/permissions-reference`
- Find tenant ID: `https://learn.microsoft.com/en-us/entra/fundamentals/how-to-find-tenant`

---

## 10. Scope estimate & recommendation

| Layer | Effort | Notes |
|-------|--------|-------|
| Models + settings + keyring + paths | Small | mechanical mirror of Google Drive |
| `configure_one_drive` (Strategy A) | Small | ~5–10 lines diff vs gdrive + a drive_id/drive_type guard |
| `configure_one_drive` (Strategy B) | Medium | only if Section 8.1 shows Strategy A is insufficient |
| mount spec / target / platform / commands / lib.rs | Small | copy-paste |
| frontend (types, html panel, main.ts, validation) | Small | copy the gdrive panel |
| tests (rust + frontend) | Small–Medium | extend existing contract/round-trip tests |

**Recommendation:** Build personal OneDrive with **Strategy A**, macOS `~/Drives/onedrive` +
Windows `O:`, region `global`, blank client_id. Gate the work behind the manual smoke test in
Section 8.1 to confirm Strategy A auto-resolves `drive_id`/`drive_type`; if not, switch
`configure_one_drive` to Strategy B using the documented state/result sequence. Leave
business/SharePoint, custom client_id, and explicit drive picking as documented follow-ups.
