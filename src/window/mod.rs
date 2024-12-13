mod imp;
use abi_stable::std_types::{RSome, RString};
use gdk::Rectangle;
use glib::{clone, Object};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib, Application};
use nadi_core::{network::Network, node::NodeInner, functions::NadiFunctions};
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

    fn setup_callbacks(&self) {
        // Setup callback for activation of the entry
        self.imp().btn_browse.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| {
                let mut dialog = gtk::FileDialog::builder()
                    .title("Nadi Network File")
                    .accept_label("Open");
                let txt = window.imp().txt_browse.text();
                if !txt.is_empty() {
                    dialog = dialog.initial_file(&gio::File::for_path(txt));
                };

                dialog.build().open(
                    Some(&window),
                    gio::Cancellable::NONE,
                    clone!(
                        #[weak]
                        window,
                        move |file| {
                            if let Ok(file) = file {
                                window.reload_file(&file).unwrap();
                            }
                        }
                    ),
                );
            }
        ));
	
        self.imp().btn_save.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| {
                window.reload_network().unwrap()
            }
        ));
    }

    pub fn reload_network(&self) -> anyhow::Result<()> {
	let buf = self.imp().tv_frame.buffer();
	let txt = buf.text(&buf.start_iter(), &buf.end_iter(), true).to_string();
	let script =
            nadi_core::parser::functions::parse_script_complete(&txt).map_err(anyhow::Error::msg)?;
	
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

    pub fn reload_file(&self, file: &gtk::gio::File) -> anyhow::Result<()> {
        let filename = file.path().expect("Couldn't get file path");
        let name = filename.to_string_lossy().to_string();
        self.imp().txt_browse.set_text(&name);
        let txt = std::fs::read_to_string(&name)?;
	self.imp().tv_frame.buffer().set_text(&txt);
	self.reload_network()
    }

    fn setup_drawing_area(&self) {
        self.imp().da_network.set_cursor_from_name(Some("pointer"));
        // self.imp().da_network.connect(
        //     "motion-notify-event",
        //     true,
        //     clone!(
        //         // #[weak(rename_to=window)]
        //         // self,
        //         move |v| {
        //             println!("{v:?}");
        //             None
        //         }
        //     ),
        // );
        self.imp().da_network.set_draw_func(move |da, ctx, w, h| {
            // network data will be available when a new network is loaded.
            // TODO, make a different network data type for graph/plots
            if let Some(net) = unsafe { da.data::<Network>("network") } {
                let net: &Network = unsafe { &*net.as_ptr() };
                ctx.set_source_rgb(0.0, 0.0, 1.0);
                ctx.set_font_size(14.0);
                let offx = 10.0;
                let offy = 10.0;
                let dely = 20.0;
                let delx = 40.0;
                let mut top = h as f64 - offy;
                let mut left = offx;
                let max_lev = net
                    .nodes()
                    .map(|n| n.lock().level())
                    .max()
                    .unwrap_or_default();

                let mut text_start = left + delx * max_lev as f64 + offx;
                let max_text = net
                    .nodes()
                    .map(|n| ctx.text_extents(n.lock().name()).unwrap().width())
                    .fold(0.0, f64::max);
                if (text_start + max_text) > w as f64 {
                    da.set_width_request((text_start + max_text).ceil() as i32);
                } else {
                    left += (w as f64 - (text_start + max_text)) / 2.0;
                    text_start += left - offx;
                }
                let req_height = (dely * (net.nodes_count() - 1) as f64) + offy * 2.0;
                if req_height > h as f64 {
                    da.set_content_height(req_height.ceil() as i32);
                } else {
                    top = (h / 2) as f64 + req_height / 2.0 - offy;
                }

                ctx.move_to(offx, offy);
                for n in net.nodes() {
                    let n = n.lock();
		    let (r, g, b) = get_node_color(&n);
		    ctx.set_source_rgb(r, g, b);
                    let nx = left + delx * n.level() as f64;
                    let ny = top - dely * n.index() as f64;
                    ctx.move_to(nx, ny);
                    let rect = Rectangle::new(nx.floor() as i32 - 5, ny.floor() as i32 - 5, 10, 10);
                    ctx.add_rectangle(&rect);
                    _ = ctx.fill();
                    if let RSome(o) = n.output() {
                        let o = o.lock();
                        ctx.move_to(nx, ny);
			let (r, g, b) = get_line_color(&n);
			ctx.set_source_rgb(r, g, b);
                        ctx.line_to(
                            left + delx * o.level() as f64,
                            top - dely * o.index() as f64,
                        );
                        _ = ctx.stroke();
                    }
                    ctx.move_to(text_start, ny);
		    let (r, g, b) = get_text_color(&n);
		    ctx.set_source_rgb(r, g, b);
		    let label = get_node_label(&n);
                    _ = ctx.show_text(&label);
                }
            }
        });
    }

    fn setup_term(&self) {
        // self.imp().term_main.connect_unsafe(signal_name, after, callback)
        let term = &self.imp().term_main;
        term.feed(">> ".as_bytes());
        unsafe { term.set_data("current_line", String::new()) };
        term.connect_commit(move |tm, ch, flag| {
            println!("{ch:?} {flag}");
            if flag != 1 {
                // todo handle other keypress than chars
                return;
            }
            match ch {
                "\r" => {
                    tm.feed("\r\n".as_bytes());
                    let line: &mut String =
                        unsafe { &mut *tm.data::<String>("current_line").unwrap().as_ptr() };
                    match line.as_str() {
                        "clear" => tm.reset(true, false),
                        _ => tm.feed(format!("Run: {line}\r\n").as_bytes()),
                    };
                    line.clear();
                    // do something
                    tm.feed(">> ".as_bytes());
                }
                _ => {
                    let line: &mut String =
                        unsafe { &mut *tm.data::<String>("current_line").unwrap().as_ptr() };
                    line.push_str(&ch);
                    tm.feed(ch.as_bytes());
                }
            };
        });
    }
}


fn get_node_color(node: &NodeInner) -> (f64, f64, f64) {
    node.try_attr::<(f64, f64, f64)>("nodecolor").unwrap_or((0.0, 0.0, 0.0))
}

fn get_node_label(node: &NodeInner) -> String {
    node.try_attr::<String>("nodelabel").unwrap_or(node.name().to_string())
}

fn get_line_color(node: &NodeInner) -> (f64, f64, f64) {
    node.try_attr::<(f64, f64, f64)>("linecolor").unwrap_or((0.0, 0.0, 0.0))
}

fn get_text_color(node: &NodeInner) -> (f64, f64, f64) {
    node.try_attr::<(f64, f64, f64)>("textcolor").unwrap_or((0.0, 0.0, 0.0))
}
