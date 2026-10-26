// Sin consola extra en Windows en las builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let started = std::time::Instant::now();
    oculus_desktop::run(started);
}
