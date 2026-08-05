#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(target_os = "linux")]
mod app_linux;
#[cfg(target_os = "windows")]
mod app_windows;

#[cfg(target_os = "linux")]
use app_linux as platform_app;
#[cfg(target_os = "windows")]
use app_windows as platform_app;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
compile_error!("Arctic ComfyUI Helper currently supports Linux and Windows targets");

fn main() {
    platform_app::run();
}
