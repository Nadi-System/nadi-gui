mod colors;
mod network;
mod window;

use gtk::gio::ApplicationFlags;
use gtk::prelude::*;
use gtk::{gio, glib, Application};
use window::Window;

fn main() -> glib::ExitCode {
    gio::resources_register_include!("nadi-gui.gresource").expect("Failed to register resources.");

    // Create a new application
    let app = Application::builder()
        .flags(ApplicationFlags::HANDLES_OPEN)
        .application_id("org.zerosofts.NadiGui")
        .build();

    // Connect to "activate" signal of `app`
    app.connect_activate(build_ui);
    app.connect_startup(|_| load_css());
    app.connect_open(|a, files, _| {
        let window = Window::new(a);
        window.open_file(&files[0]).unwrap();
        window.present();
    });
    set_accels(&app);

    // Run the application
    let args: Vec<String> = std::env::args().collect();
    app.run_with_args(&args)
}

fn set_accels(app: &Application) {
    app.set_accels_for_action("win.close", &["<Ctrl>W"]);
    app.set_accels_for_action("win.open", &["<Ctrl>O"]);
    app.set_accels_for_action("win.save", &["<Ctrl>S"]);
    app.set_accels_for_action("win.export", &["<Ctrl>E"]);
}

fn build_ui(app: &Application) {
    // Create a new custom window and present it
    let window = Window::new(app);
    window.present();
}

fn load_css() {
    // Load the CSS file and add it to the provider
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        //         "
        // window {
        //   background-color: transparent;
        // }
        // "
        "
drawingarea {
  background-color: red;
  border-radius: 10px;
  padding: 5px;
}

",
    );

    // Add the provider to the default screen
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
