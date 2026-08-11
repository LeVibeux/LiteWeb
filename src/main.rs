mod adblock;
mod app;
mod benchmark;
mod browser;
mod commands;
mod energy;
mod storage;
mod ui;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("LiteWeb: {error}");
        std::process::exit(2);
    }
}
