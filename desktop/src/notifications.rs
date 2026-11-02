use tauri::AppHandle;
use tauri_plugin_notification::{NotificationExt, PermissionState};

fn ensure_notification_permission(app: &AppHandle) -> bool {
    match app.notification().permission_state() {
        Ok(PermissionState::Granted) => true,
        Ok(PermissionState::Prompt | PermissionState::PromptWithRationale) => {
            matches!(
                app.notification().request_permission(),
                Ok(PermissionState::Granted)
            )
        }
        Ok(PermissionState::Denied) | Err(_) => false,
    }
}

pub fn show_app_notification(app: &AppHandle, body: &str) {
    if !ensure_notification_permission(app) {
        return;
    }

    let _ = app
        .notification()
        .builder()
        .title("Cloud Drive Mount")
        .body(body)
        .show();
}
