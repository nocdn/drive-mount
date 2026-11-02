//! System notifications for work that happens while the window may be closed
//! (mounting at launch, unmounting before quitting).

pub const APP_TITLE: &str = "Cloud Drive Mount";
#[cfg(target_os = "macos")]
const BUNDLE_IDENTIFIER: &str = "com.bartek.clouddrivemount";

/// Shows `body` under the app's name. Failures (notifications turned off,
/// no notification service) are ignored: everything shown here is also in
/// the log.
pub fn show_app_notification(body: &str) {
    #[cfg(target_os = "macos")]
    {
        static APPLICATION: std::sync::Once = std::sync::Once::new();
        APPLICATION.call_once(|| {
            let _ = notify_rust::set_application(BUNDLE_IDENTIFIER);
        });
    }
    let _ = notify_rust::Notification::new()
        .summary(APP_TITLE)
        .body(body)
        .show();
}
