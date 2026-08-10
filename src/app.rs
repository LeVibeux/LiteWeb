use gtk::prelude::*;
use gtk::Application;

use crate::ui::BrowserWindow;

const APP_ID: &str = "com.liteweb.browser";

pub fn run() {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        let window = BrowserWindow::new(app);
        window.show_all();
    });

    app.run();
}
