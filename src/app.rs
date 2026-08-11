use gtk::prelude::*;
use gtk::Application;

use crate::benchmark::BenchmarkConfig;
use crate::ui::BrowserWindow;

const APP_ID: &str = "com.liteweb.browser";

pub fn run() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let benchmark = BenchmarkConfig::from_args(&args)?;
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(move |app| {
        let window = BrowserWindow::new(app, benchmark.clone());
        window.show_all();
    });

    let gtk_args = ["liteweb"];
    let status = app.run_with_args(&gtk_args);
    if status == 0 {
        Ok(())
    } else {
        Err(format!("GTK application exited with status {status}"))
    }
}
