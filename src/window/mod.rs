mod imp;
use super::network;
use abi_stable::std_types::{RSome, RString};
use colored::Colorize;
use gdk::Rectangle;
use gio::ActionEntry;
use glib::{clone, Object};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib, Application};
use nadi_core::{functions::NadiFunctions, network::Network, node::NodeInner};
use std::fs::File;
use std::io::{Read, Write};
use std::iter::Iterator;
use vte4::prelude::*;

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends gtk::ApplicationWindow, gtk::Window, gtk::Widget,
    @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager, gtk::EventControllerMotion;
}

impl Window {
    pub fn new(app: &Application) -> Self {
        // Create new window
        Object::builder().property("application", app).build()
    }

    fn setup_data(&self) {
        let net = Network::default();
        let funcs = NadiFunctions::new();
        unsafe {
            self.imp().da_network.set_data("network", net);
            self.imp().da_network.set_data("functions", funcs);
        }
    }

    fn setup_actions(&self) {
        // Add action "close" to `window` taking no parameter
        let action_close = ActionEntry::builder("close")
            .activate(|window: &Window, _, _| {
                window.close();
            })
            .build();
        let action_open = ActionEntry::builder("open")
            .activate(|window: &Window, _, _| {
                window.open();
            })
            .build();
        let action_save = ActionEntry::builder("save")
            .activate(|window: &Window, _, _| {
                let _ = window.save_file();
            })
            .build();
        let action_export = ActionEntry::builder("export")
            .activate(|window: &Window, _, _| {
                window.export();
            })
            .build();
        self.add_action_entries([action_open, action_close, action_save, action_export]);
    }

    fn setup_callbacks(&self) {
        // Setup callback for activation of the entry
        self.imp().btn_browse.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| {
                window.open();
            }
        ));

        self.imp().btn_save.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| { window.save_file().unwrap() }
        ));
    }

    pub fn open(&self) {
        let mut dialog = gtk::FileDialog::builder()
            .title("Nadi Network File")
            .accept_label("Open");
        let txt = self.imp().txt_browse.text();
        if !txt.is_empty() {
            dialog = dialog.initial_file(&gio::File::for_path(txt));
        };

        dialog.build().open(
            Some(&self.clone()),
            gio::Cancellable::NONE,
            clone!(
                #[weak(rename_to=window)]
                self,
                move |file| {
                    if let Ok(file) = file {
                        window.open_file(&file).unwrap();
                    }
                }
            ),
        );
    }

    pub fn export(&self) {
        let mut dialog = gtk::FileDialog::builder()
            .title("Export File")
            .accept_label("Save");
        let txt = self.imp().txt_browse.text();
        if !txt.is_empty() {
            dialog = dialog.initial_file(&gio::File::for_path(txt));
        };

        dialog.build().save(
            Some(&self.clone()),
            gio::Cancellable::NONE,
            clone!(
                #[weak(rename_to=window)]
                self,
                move |file| {
                    if let Ok(file) = file {
                        window.export_file(&file);
                    }
                }
            ),
        );
    }

    pub fn reload_network(&self) -> anyhow::Result<()> {
        let buf = self.imp().tv_frame.buffer();
        let txt = buf
            .text(&buf.start_iter(), &buf.end_iter(), true)
            .to_string();
        let script = nadi_core::parser::functions::parse_script_complete(&txt)
            .map_err(anyhow::Error::msg)?;

        let functions = NadiFunctions::new();
        let mut net = Network::default();
        for fc in &script {
            functions
                .execute(fc, &mut net)
                .map_err(anyhow::Error::msg)?;
        }
        unsafe {
            self.imp().da_network.set_data("network", net);
        }
        self.imp().da_network.queue_draw();
        Ok(())
    }

    pub fn save_file(&self) -> anyhow::Result<()> {
        let name = self.imp().txt_browse.text();
        let buf = self.imp().tv_frame.buffer();
        let txt = buf
            .text(&buf.start_iter(), &buf.end_iter(), true)
            .to_string();
        let mut file = File::create(&name)?;
        file.write_all(txt.as_bytes())?;
        self.reload_network()
    }

    pub fn open_file(&self, file: &gtk::gio::File) -> anyhow::Result<()> {
        let filename = file.path().expect("Couldn't get file path");
        let name = filename.to_string_lossy().to_string();
        self.imp().txt_browse.set_text(&name);
        let txt = std::fs::read_to_string(&name)?;
        self.imp().tv_frame.buffer().set_text(&txt);
        self.reload_network()
    }

    pub fn export_file(&self, file: &gtk::gio::File) {
        let filename = file.path().expect("Couldn't get file path");
        let name = filename.to_string_lossy().to_string();
        if let Some(net) = unsafe { self.imp().da_network.data::<Network>("network") } {
            let net: &Network = unsafe { &*net.as_ptr() };
            let mut svg = cairo::SvgSurface::new::<&str>(400.0, 500.0, None).unwrap();
            let ctx = cairo::Context::new(&mut svg).unwrap();
            let (mut h, mut w) = network::calc_hw(net, &ctx);
            h += 50;
            w += 50;
            let ext: &str = name.as_str().split('.').last().unwrap();
            match ext {
                "svg" => {
                    let mut svg = cairo::SvgSurface::new(w as f64, h as f64, Some(name)).unwrap();
                    let ctx = cairo::Context::new(&mut svg).unwrap();
                    network::draw_network(net, &ctx, w, h, None);
                }
                "pdf" => {
                    let mut pdf = cairo::PdfSurface::new(w as f64, h as f64, name).unwrap();
                    let ctx = cairo::Context::new(&mut pdf).unwrap();
                    network::draw_network(net, &ctx, w, h, None);
                }
                "png" => {
                    let mut png = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).unwrap();
                    let ctx = cairo::Context::new(&mut png).unwrap();
                    network::draw_network(net, &ctx, w, h, None);
                    let mut f = File::create(name).unwrap();
                    png.write_to_png(&mut f).unwrap();
                }
                _ => (),
            }
        }
    }

    fn setup_drawing_area(&self) {
        self.imp().da_network.set_cursor_from_name(Some("pointer"));
        self.imp().da_network.set_draw_func(move |da, ctx, w, h| {
            // network data will be available when a new network is loaded.
            // TODO, make a different network data type for graph/plots
            if let Some(net) = unsafe { da.data::<Network>("network") } {
                let net: &Network = unsafe { &*net.as_ptr() };
                network::draw_network(net, ctx, w, h, Some(da));
            }
        });
    }

    fn setup_term(&self) {
        let term = &self.imp().term_main;
        term.feed(">> ".as_bytes());
        unsafe { term.set_data("current_line", String::new()) };
        let da = &self.imp().da_network;
        term.connect_commit(clone!(
            #[weak]
            da,
            move |tm, ch, flag| {
                if flag != 1 {
                    // todo handle other keypress than chars
                    return;
                }
                match ch {
                    "\r" => {
                        tm.feed("\r\n".as_bytes());
                        let line: &mut String =
                            unsafe { &mut *tm.data::<String>("current_line").unwrap().as_ptr() };
                        match line.trim() {
                            "" => (),
                            "clear" => tm.reset(true, false),
                            _ => {
                                run_task(tm, &da, format!("{line}\n"));
                                tm.feed("\r\n".as_bytes());
                                // since the task could have changed the network properties
                                da.queue_draw();
                            }
                        };
                        line.clear();
                        tm.feed(">> ".as_bytes());
                    }
                    _ => {
                        let line: &mut String =
                            unsafe { &mut *tm.data::<String>("current_line").unwrap().as_ptr() };
                        line.push_str(&ch);
                        tm.feed(ch.as_bytes());
                    }
                };
            }
        ));
    }
}

fn run_task(term: &vte4::Terminal, darea: &gtk::DrawingArea, line: String) {
    let funcs = if let Some(func) = unsafe { darea.data::<NadiFunctions>("functions") } {
        let funcs: &NadiFunctions = unsafe { &*func.as_ptr() };
        funcs
    } else {
        term.feed("No network set".as_bytes());
        return;
    };
    let mut skin = termimad::MadSkin::default_dark();
    for h in &mut skin.headers {
        h.align = termimad::Alignment::Left;
    }
    if let Some(cmd) = line.strip_prefix("help") {
        let cmd = cmd.trim();
        if let Some((n, c)) = cmd.split_once(' ') {
            match n {
                "node" => {
                    if let Some(f) = funcs.node(c) {
                        push_func_help(&skin, term, "node", &f.name(), &f.signature(), &f.help());
                    } else {
                        term.feed("Node Function Not Found".as_bytes());
                    }
                }
                "network" => {
                    if let Some(f) = funcs.network(c) {
                        push_func_help(
                            &skin,
                            term,
                            "network",
                            &f.name(),
                            &f.signature(),
                            &f.help(),
                        );
                    } else {
                        term.feed("Node Function Not Found".as_bytes());
                    }
                }
                _ => term.feed("Invalid help subcommand use node or network".as_bytes()),
            };
        } else {
            if let Some(f) = funcs.node(cmd) {
                push_func_help(&skin, term, "node", &f.name(), &f.signature(), &f.help());
            }
            if let Some(f) = funcs.network(cmd) {
                push_func_help(&skin, term, "network", &f.name(), &f.signature(), &f.help());
            }
        }
        return;
    }
    let network = if let Some(net) = unsafe { darea.data::<Network>("network") } {
        let net: &mut Network = unsafe { &mut *net.as_ptr() };
        net
    } else {
        term.feed("No network set".as_bytes());
        return;
    };
    let script = match nadi_core::parser::functions::parse_script_complete(&line) {
        Ok(t) => t,
        Err(e) => {
            term.feed(format!("Error: {e:?}").as_bytes());
            return;
        }
    };

    // temp solution, make NadiFunctions take a std::io::Write or
    // other trait object that can either print to stdout, or take the
    // result to show somewhere else (like here)
    let mut buf = gag::BufferRedirect::stdout().unwrap();
    let mut output = String::new();
    for fc in &script {
        term.feed(format!("#> {}\r\n", fc.to_colored_string()).as_bytes());
        let res = funcs.execute(fc, network);
        // print the stdout output to the terminal
        buf.read_to_string(&mut output).unwrap();
        term.feed(output.replace("\n", "\r\n").as_bytes());
        output.clear();
        if let Err(e) = res {
            term.feed(format!("Error: {e}").as_bytes());
            return;
        }
    }
}

fn push_func_help(
    skin: &termimad::MadSkin,
    term: &vte4::Terminal,
    ty: &str,
    name: &str,
    sig: &str,
    help: &str,
) {
    term.feed(
        format!(
            "{} {} {}\r\n",
            ty.red(),
            name.truecolor(80, 80, 200),
            sig.blue(),
        )
        .as_bytes(),
    );
    let txt = skin.text(help, Some(term.width() as usize));
    term.feed(txt.to_string().replace("\n", "\r\n").as_bytes());
    term.feed("\r\n".as_bytes());
}
