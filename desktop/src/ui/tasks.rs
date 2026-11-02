//! What the window's buttons do. Anything that touches rclone, the keychain
//! or the network runs on a worker thread; its result comes back as an
//! update on the UI thread. Without a backend (scenes, tests) jobs don't
//! run, so fixtures stay as they were drawn.

use std::path::PathBuf;

use crate::backend::{self, Backend};
use crate::models::{B2Credentials, CloudProvider, MountRequest, MountState};

use super::form::{seedbox_in_use, validate_b2, validate_seedbox, Field, MountSnapshot};
use super::{Operation, Pane, Task, Ui};

impl Ui {
    /// Runs `job` off the UI thread, then `done` with its result on it.
    fn run<R: Send + 'static>(
        &mut self,
        job: impl FnOnce(&Backend) -> R + Send + 'static,
        done: impl FnOnce(&mut Ui, R) + Send + 'static,
    ) {
        let (Some(backend), Some(window)) = (self.backend.clone(), self.window.clone()) else {
            return;
        };
        std::thread::spawn(move || {
            let result = job(&backend);
            window.update(move |ui, _| done(ui, result));
        });
    }

    /// The launch sequence: clean up what a previous run left, unlock the
    /// saved credentials, then mount what was mounted before.
    pub(super) fn launch(&mut self) {
        let Some(backend) = &self.backend else {
            return;
        };
        if !crate::rclone::is_fuse_installed() {
            self.fuse_installed = Some(false);
            backend.log_error(if self.platform == super::Platform::Mac {
                "FUSE-T is not installed."
            } else {
                "WinFsp is not installed."
            });
        } else {
            self.fuse_installed = Some(true);
        }
        backend.log_info(
            "Cloud Drive Mount started. Use the menu bar or tray icon to reopen Settings after closing this window.",
        );
        let start_at_login = self.form.start_at_login;
        self.operation = Some(Operation::Mounting);
        let window = self.window.clone();
        self.run(
            move |backend| {
                if let Err(err) = crate::autostart::apply(start_at_login) {
                    backend.log_error(format!("Start at login update failed: {err}"));
                }
                backend.startup_cleanup();
                let credentials = backend.load_credentials();
                let unlocked = credentials.is_ok();
                if let Some(window) = &window {
                    window.update(move |ui, _| match credentials {
                        Ok(loaded) => ui.set_credentials(&loaded),
                        Err(err) => {
                            ui.credentials.unlock_failed = true;
                            ui.log_error(&err);
                        }
                    });
                }
                if unlocked {
                    backend.attempt_auto_mount_once();
                }
                backend.rclone.mount_state()
            },
            |ui, state| {
                ui.operation = None;
                ui.set_mount_state(state);
            },
        );
    }

    /// Saves the settings as the fields show them, and applies start at
    /// login. Secrets are saved only by the actions that use them.
    pub(crate) fn save_preferences(&mut self) {
        let settings = self.form.settings(self.platform);
        self.run(
            move |_| {
                let start_at_login = settings.start_at_login;
                backend::save_settings_normalized(settings)?;
                crate::autostart::apply(start_at_login)
                    .map_err(|err| format!("Start at login update failed: {err}"))
            },
            |ui, result: Result<(), String>| {
                if let Err(err) = result {
                    ui.log_error(&err);
                }
            },
        );
    }

    fn start(&mut self, operation: Operation) -> bool {
        if self.operation.is_some() {
            return false;
        }
        self.flush_edits();
        self.failure = None;
        self.confirmation = None;
        self.operation = Some(operation);
        true
    }

    fn finish(&mut self, state: MountState) {
        self.operation = None;
        self.set_mount_state(state);
    }

    /// "Save and mount all": checks the fields, saves them, and (re)mounts
    /// everything configured.
    pub(crate) fn mount_all(&mut self) {
        if self.operation.is_some() || (self.mounted() && !self.has_mount_changes()) {
            return;
        }
        let mut errors = validate_b2(&self.form, self.platform, self.credentials.has_b2);
        if seedbox_in_use(&self.form) {
            self.form.normalize_seedbox_host();
            errors.extend(validate_seedbox(
                &self.form,
                false,
                self.credentials.seedbox_password,
            ));
        }
        if !errors.is_empty() {
            let pane = first_pane_with(&errors);
            self.errors = errors;
            self.fail(
                "Check the highlighted fields",
                "Fix the highlighted fields before mounting.",
            );
            self.pane = pane;
            return;
        }
        self.errors.clear();
        if !self.start(Operation::Mounting) {
            return;
        }
        let settings = self.form.settings(self.platform);
        let key_id = self.form.b2_key_id.text.trim().to_string();
        let key = self.form.b2_key.text.trim().to_string();
        let request = MountRequest {
            application_key_id: key_id.clone(),
            application_key: key.clone(),
            buckets: settings.buckets.clone(),
            google_drive: settings.google_drive.clone(),
            one_drive: settings.one_drive.clone(),
            seedbox: settings.seedbox.clone(),
            seedbox_password: self.form.seedbox_password.text.clone(),
            selected_provider: settings.selected_provider,
        };
        let snapshot = MountSnapshot::of(&settings, self.platform);
        let typed_key = !key_id.is_empty() || !key.is_empty();
        self.run(
            move |backend| {
                let saved = backend::save_settings_normalized(settings.clone()).and_then(|_| {
                    if typed_key {
                        backend::save_b2_credentials_now(&B2Credentials {
                            application_key_id: key_id,
                            application_key: key,
                        })
                    } else {
                        Ok(())
                    }
                });
                if let Err(err) = crate::autostart::apply(settings.start_at_login) {
                    backend.log_error(format!("Start at login update failed: {err}"));
                }
                let result = saved.and_then(|_| backend.mount_all(request));
                (result, backend.rclone.mount_state())
            },
            move |ui, (result, state)| {
                ui.finish(state);
                match result {
                    Ok(()) => {
                        if typed_key {
                            ui.credentials.has_b2 = true;
                        }
                        ui.active = Some(snapshot);
                    }
                    Err(err) => ui.fail("Couldn't mount your drives", err),
                }
            },
        );
    }

    pub(crate) fn unmount_all(&mut self) {
        if !self.mounted() || !self.start(Operation::Unmounting) {
            return;
        }
        self.run(
            |backend| {
                backend.unmount_all();
                backend.rclone.mount_state()
            },
            |ui, state| ui.finish(state),
        );
    }

    pub(crate) fn refresh(&mut self) {
        if !self.mounted() || !self.start(Operation::Refreshing) {
            return;
        }
        self.run(
            |backend| (backend.refresh_mount_caches(), backend.rclone.mount_state()),
            |ui, (result, state)| {
                ui.finish(state);
                if let Err(err) = result {
                    ui.fail("Couldn't refresh the folder listings", err);
                }
            },
        );
    }

    pub(crate) fn restart(&mut self) {
        let settings = self.form.settings(self.platform);
        if !self.start(Operation::Restarting) {
            return;
        }
        self.run(
            move |backend| {
                let result = backend::save_settings_normalized(settings)
                    .and_then(|_| backend.restart_mounts());
                (result, backend.rclone.mount_state())
            },
            |ui, (result, state)| {
                ui.active = None;
                ui.finish(state);
                if let Err(err) = result {
                    ui.fail("Couldn't restart the mounts", err);
                }
            },
        );
    }

    fn begin(&mut self, task: Task) -> bool {
        if !self.tasks.insert(task) {
            return false;
        }
        self.failure = None;
        self.confirmation = None;
        true
    }

    fn end(&mut self, task: Task, result: Result<String, String>, failed: &str) {
        self.tasks.remove(&task);
        match result {
            Ok(confirmation) => {
                if !confirmation.is_empty() {
                    self.confirmation = Some((task, confirmation));
                }
            }
            Err(err) => self.fail(failed, err),
        }
    }

    pub(crate) fn toggle_google_drive(&mut self) {
        if self.busy(Task::TestGoogle) || !self.begin(Task::ConnectGoogle) {
            return;
        }
        self.form.selected_provider = CloudProvider::GoogleDrive;
        let mut settings = self.form.settings(self.platform);
        settings.selected_provider = CloudProvider::GoogleDrive;
        let connected = self.credentials.google_drive;
        self.run(
            move |backend| {
                let google_drive = settings.google_drive.clone();
                backend::save_settings_normalized(settings)?;
                if connected {
                    backend.disconnect_google_drive(google_drive)?;
                    backend.log_info("Google Drive is disconnected.");
                } else {
                    backend.log_info(
                        "Starting Google Drive authorization. Complete the sign-in in your browser.",
                    );
                    backend.configure_google_drive(google_drive)?;
                    backend.log_info(
                        "Google Drive is connected. Use Save and mount all to mount it.",
                    );
                }
                Ok(())
            },
            move |ui, result: Result<(), String>| {
                if result.is_ok() {
                    ui.credentials.google_drive = !connected;
                }
                let failed = if connected {
                    "Couldn't disconnect Google Drive"
                } else {
                    "Couldn't connect Google Drive"
                };
                ui.end(Task::ConnectGoogle, result.map(|_| String::new()), failed);
            },
        );
    }

    pub(crate) fn test_google_drive(&mut self) {
        if self.busy(Task::ConnectGoogle) || !self.begin(Task::TestGoogle) {
            return;
        }
        let settings = self.form.settings(self.platform);
        self.run(
            move |backend| {
                backend.test_google_drive(settings.google_drive)?;
                backend.log_info("Google Drive connection test succeeded.");
                Ok("Connection works".to_string())
            },
            |ui, result| {
                ui.end(
                    Task::TestGoogle,
                    result,
                    "Google Drive connection test failed",
                )
            },
        );
    }

    pub(crate) fn toggle_one_drive(&mut self) {
        if self.busy(Task::TestOneDrive) || !self.begin(Task::ConnectOneDrive) {
            return;
        }
        self.form.selected_provider = CloudProvider::OneDrive;
        let mut settings = self.form.settings(self.platform);
        settings.selected_provider = CloudProvider::OneDrive;
        let connected = self.credentials.one_drive;
        self.run(
            move |backend| {
                let one_drive = settings.one_drive.clone();
                backend::save_settings_normalized(settings)?;
                if connected {
                    backend.disconnect_one_drive(one_drive)?;
                    backend.log_info("OneDrive is disconnected.");
                } else {
                    backend.log_info(
                        "Starting OneDrive authorization. Complete the sign-in in your browser.",
                    );
                    backend.configure_one_drive(one_drive)?;
                    backend.log_info("OneDrive is connected. Use Save and mount all to mount it.");
                }
                Ok(())
            },
            move |ui, result: Result<(), String>| {
                if result.is_ok() {
                    ui.credentials.one_drive = !connected;
                }
                let failed = if connected {
                    "Couldn't disconnect OneDrive"
                } else {
                    "Couldn't connect OneDrive"
                };
                ui.end(Task::ConnectOneDrive, result.map(|_| String::new()), failed);
            },
        );
    }

    pub(crate) fn test_one_drive(&mut self) {
        if self.busy(Task::ConnectOneDrive) || !self.begin(Task::TestOneDrive) {
            return;
        }
        let settings = self.form.settings(self.platform);
        self.run(
            move |backend| {
                backend.test_one_drive(settings.one_drive)?;
                backend.log_info("OneDrive connection test succeeded.");
                Ok("Connection works".to_string())
            },
            |ui, result| {
                ui.end(
                    Task::TestOneDrive,
                    result,
                    "OneDrive connection test failed",
                )
            },
        );
    }

    pub(crate) fn test_seedbox(&mut self) {
        if self.busy(Task::ForgetSeedbox) || self.busy(Task::TestSeedbox) {
            return;
        }
        self.form.normalize_seedbox_host();
        let errors = validate_seedbox(&self.form, true, self.credentials.seedbox_password);
        self.errors.retain(|field, _| !is_seedbox_field(*field));
        if !errors.is_empty() {
            self.errors.extend(errors);
            self.fail(
                "Check the highlighted fields",
                "Fix the highlighted Seedbox fields before continuing.",
            );
            return;
        }
        self.begin(Task::TestSeedbox);
        let settings = self.form.settings(self.platform);
        let password = self.form.seedbox_password.text.clone();
        self.run(
            move |backend| {
                backend.test_seedbox(settings.seedbox, password)?;
                backend.log_info("Seedbox connection test succeeded.");
                Ok("Connection works".to_string())
            },
            |ui, result: Result<String, String>| {
                if result.is_ok() {
                    ui.form.seedbox_password.replace_text("");
                    ui.credentials.seedbox = true;
                    ui.credentials.seedbox_password = true;
                }
                ui.end(Task::TestSeedbox, result, "Seedbox connection test failed");
            },
        );
    }

    pub(crate) fn forget_seedbox(&mut self) {
        if self.busy(Task::TestSeedbox) || !self.begin(Task::ForgetSeedbox) {
            return;
        }
        let settings = self.form.settings(self.platform);
        self.run(
            move |backend| {
                let seedbox = settings.seedbox.clone();
                backend::save_settings_normalized(settings)?;
                backend.forget_seedbox(seedbox)?;
                backend.log_info("Seedbox is disconnected.");
                Ok(String::new())
            },
            |ui, result: Result<String, String>| {
                if result.is_ok() {
                    ui.form.seedbox_password.replace_text("");
                    ui.credentials.seedbox = false;
                    ui.credentials.seedbox_password = false;
                }
                ui.end(Task::ForgetSeedbox, result, "Couldn't forget the Seedbox");
            },
        );
    }

    pub(crate) fn clear_logs(&mut self) {
        self.logs.clear();
        if let Some(backend) = &self.backend {
            if let Err(err) = backend.clear_logs() {
                self.push_line(crate::models::LogLine {
                    level: "ERROR".into(),
                    message: format!("Couldn't clear the log file: {err}"),
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                });
            }
        }
    }

    pub(crate) fn open_target(&mut self, target: String) {
        if self.backend.is_none() {
            return;
        }
        if let Err(err) = backend::open_path(&PathBuf::from(&target)) {
            self.fail("Couldn't open the drive", err);
        }
    }
}

fn is_seedbox_field(field: Field) -> bool {
    matches!(
        field,
        Field::SeedboxHost | Field::SeedboxPort | Field::SeedboxUser | Field::SeedboxPassword
    )
}

/// The pane that shows the first field with an error.
fn first_pane_with(errors: &super::form::Errors) -> Pane {
    match errors.keys().next() {
        Some(field) if is_seedbox_field(*field) => Pane::Seedbox,
        Some(_) => Pane::B2,
        None => Pane::Drives,
    }
}
