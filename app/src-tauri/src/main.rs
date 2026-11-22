// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Started as administrator only to create one game folder (spec §6k): do that and exit,
    // before anything else of the app starts.
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if fishy_core::install::elevate::is_helper_call(&args) {
        std::process::exit(fishy_core::install::elevate::run_helper(&args));
    }
    fishy_client_lib::run()
}
