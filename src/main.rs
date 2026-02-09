use arctic_downloader::app::ArcticDownloaderApp;
use env_logger::Env;
use slint::fontique_07::fontique;

fn register_inter_font() {
    const INTER_FONT: &[u8] = include_bytes!("../assets/fonts/Inter-VariableFont_opsz,wght.ttf");
    let blob = fontique::Blob::new(std::sync::Arc::new(INTER_FONT.to_vec()));
    let mut collection = slint::fontique_07::shared_collection();
    let _ = collection.register_fonts(blob, None);
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    register_inter_font();

    if std::env::var("SLINT_STYLE").is_err() {
        // Prefer Fluent controls on Windows 11 while still allowing overrides.
        std::env::set_var("SLINT_STYLE", "fluent");
    }

    ArcticDownloaderApp::new()?.run()
}
