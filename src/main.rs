use adw::{glib, gtk, ColorScheme, StyleManager};
use arctic_downloader::app::{ArcticDownloaderApp, APP_ID};
use env_logger::Env;
fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    glib::log_set_handler(
        Some("Gtk"),
        glib::LogLevels::LEVEL_WARNING,
        false,
        false,
        |_domain, _level, _message| {},
    );
    glib::log_set_handler(
        Some("Gdk"),
        glib::LogLevels::LEVEL_WARNING,
        false,
        false,
        |_domain, _level, _message| {},
    );
    gtk::init()?;
    gtk::Window::set_default_icon_name(APP_ID);
    StyleManager::default().set_color_scheme(ColorScheme::Default);
    glib::set_application_name("Arctic Downloader");
    ArcticDownloaderApp::new()?.run()
}
