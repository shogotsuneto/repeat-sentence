mod app;
mod audio;
mod library;
mod model;
mod practice;
mod sentences;
mod settings_tab;
mod storage;
mod ui;
mod voices;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
