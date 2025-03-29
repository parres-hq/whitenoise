use crate::whitenoise::Whitenoise;

pub mod accounts;
pub mod groups;
pub mod invites;
pub mod key_packages;
pub mod media;
pub mod messages;
pub mod nostr;
pub mod payments;

#[tauri::command]
pub async fn delete_all_data(wn: tauri::State<'_, Whitenoise>) -> Result<(), String> {
    wn.delete_all_data().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Determines if the current platform is a mobile device.
///
/// This function checks if the application is running on either Android or iOS.
/// It uses conditional compilation to determine the target operating system.
///
/// # Returns
///
/// * `true` if running on Android or iOS
/// * `false` if running on any other platform
#[tauri::command]
pub fn is_mobile() -> bool {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    return true;

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return false;
}

/// Returns the current platform identifier as a string.
///
/// This function is a Tauri command that determines the operating system platform
/// the application is running on. It uses conditional compilation to return
/// different values based on the target platform.
///
/// # Returns
///
/// A string representing the platform:
/// - `"android"` when running on Android
/// - `"ios"` when running on iOS
/// - `"desktop"` when running on any other platform (Windows, macOS, Linux)
#[tauri::command]
pub fn is_platform() -> String {
    #[cfg(target_os = "android")]
    return "android".to_string();

    #[cfg(target_os = "ios")]
    return "ios".to_string();

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return "desktop".to_string();
}
