// Sin consola en Windows en las versiones release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    record_mind_app::run()
}
