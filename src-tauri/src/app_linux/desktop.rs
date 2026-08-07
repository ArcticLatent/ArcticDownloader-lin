//! Linux desktop-integration details kept out of the command wiring module.

use std::sync::OnceLock;
use tauri::image::Image;

pub(crate) fn main_window_icon() -> Option<Image<'static>> {
    static MAIN_ICON: OnceLock<Option<Image<'static>>> = OnceLock::new();
    MAIN_ICON
        .get_or_init(|| {
            Image::from_bytes(include_bytes!("../../icons/icon.png"))
                .ok()
                .or_else(|| Image::from_bytes(include_bytes!("../../icons/favicon.ico")).ok())
                .or_else(|| Image::from_bytes(include_bytes!("../../icons/icon.ico")).ok())
        })
        .clone()
}

/// Suppresses two known toolkit warnings that are emitted by GTK/AppIndicator
/// internals and do not describe an application failure.
pub(crate) fn install_linux_gdk_log_filter() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    if INSTALLED.get().is_some() {
        return;
    }

    glib::log_set_writer_func(|level, fields| {
        let mut domain: Option<&str> = None;
        let mut message: Option<&str> = None;
        for field in fields {
            match field.key() {
                "GLIB_DOMAIN" => domain = field.value_str(),
                "MESSAGE" => message = field.value_str(),
                _ => {}
            }
        }

        if matches!(level, glib::LogLevel::Critical)
            && domain == Some("Gdk")
            && message
                .map(|value| value.contains("gdk_window_thaw_toplevel_updates"))
                .unwrap_or(false)
        {
            return glib::LogWriterOutput::Handled;
        }

        if matches!(level, glib::LogLevel::Warning)
            && domain == Some("libayatana-appindicator")
            && message
                .map(|value| {
                    value.contains("libayatana-appindicator is deprecated")
                        && value.contains("libayatana-appindicator-glib")
                })
                .unwrap_or(false)
        {
            return glib::LogWriterOutput::Handled;
        }

        glib::log_writer_default(level, fields)
    });

    let _ = INSTALLED.set(());
}
