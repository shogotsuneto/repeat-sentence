mod app;
mod audio;
mod diag;
mod history;
mod history_tab;
mod kokoro;
mod library;
mod model;
mod panic_hook;
mod practice;
mod sentences;
mod settings_tab;
mod storage;
mod ui;
mod voices;

fn main() {
    panic_hook::install();
    leptos::mount::mount_to_body(app::App);
}
