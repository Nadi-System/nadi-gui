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
use itertools::Itertools;
use nadi_core::{tasks::TaskContext, functions::NadiFunctions, network::Network, node::NodeInner};
use nadi_core::parser::tokenizer::TaskToken;
use nadi_core::parser::NadiError;
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
        unsafe {
            self.imp().da_network.set_data("tasks_ctx", TaskContext::new(None));
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

        self.imp().btn_export.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| { window.export() }
        ));

        self.imp()
            .tv_frame
            .buffer()
            .connect_changed(clone!(@weak self as window => move |_| {
            let tb = window.imp().tv_frame.buffer();
		// todo, only do this for current line
		tb.remove_all_tags(&tb.start_iter(), &tb.end_iter());
                window.format_task(&tb);
            }));

        self.imp().btn_run.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| {
                let buf = window.imp().tv_frame.buffer();
                let tm = &window.imp().term_main;
                let mut mark = buf.iter_at_mark(&buf.selection_bound());
                let mut ins = buf.iter_at_mark(&buf.get_insert());
                if mark == ins {
                    mark = buf.start_iter();
                    ins = buf.end_iter();
                }
                let selection = buf.text(&ins, &mark, true);
                run_task(tm, &window.imp().da_network, format!("{}\n", selection.trim()));
                term_prompt(&tm);
                // since the task could have changed the network properties
                window.imp().da_network.queue_draw();
            }
        ));
    }

    fn format_task(&self, tb: &gtk::TextBuffer) {
	let mut point = tb.start_iter();
	let text = tb.text(&point, &tb.end_iter(), true);
	for line in text.lines() {
	    let mut l = point;
	    l.forward_line();
	    if let Ok(tags) = nadi_core::parser::tokenizer::get_tokens(line){
		for t in tags {
		    let st = point;
		    point.forward_chars(t.content.len() as i32);
		    let tg = match t.ty {
			TaskToken::Comment => "comment",
			TaskToken::Keyword(_) => "keyword",
			TaskToken::Function  => "function",
			TaskToken::Variable => "variable",
			TaskToken::String(_) => "string",
			TaskToken::Integer | TaskToken::Float => "number",
			TaskToken::Date | TaskToken::Time | TaskToken::DateTime => "datetime",
			TaskToken::NewLine | TaskToken::WhiteSpace => continue,
			TaskToken::PathSep => "pathsep",
			TaskToken::Assignment => "equal",
			_ => continue,
		    };
		    tb.apply_tag_by_name(tg, &st, &point);
		}
	    } else {
		tb.apply_tag_by_name("error", &point, &l);
	    }
	    point = l;
	}
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
            dialog = dialog.initial_file(&gio::File::for_path("export.pdf"));
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
	let tm = &self.imp().term_main;
	let mut tasks_ctx = TaskContext::new(None);
	// for fc in tasks {
        //     match tasks_ctx.execute(fc) {
	// 	Ok(Some(p)) => tm.feed(p.replace("\n", "\r\n").as_bytes()),
	// 	Err(p) => {
	// 	    tm.feed(p.replace("\n", "\r\n").as_bytes());
	// 	    break;
	// 	},
	// 	_ => (),
	//     }
	// }
        unsafe {
            self.imp().da_network.set_data("tasks_ctx", tasks_ctx);
        }
	run_task(tm, &self.imp().da_network, format!("{txt}\n"));
        term_prompt(&tm);
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
	if let Some(tctx) = unsafe { self.imp().da_network.data::<TaskContext>("tasks_ctx") } {
            let tctx: &mut TaskContext = unsafe { &mut *tctx.as_ptr() };
            let net: &Network = &tctx.network;
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
            if let Some(tctx) = unsafe { da.data::<TaskContext>("tasks_ctx") } {
                let tctx: &TaskContext = unsafe { &*tctx.as_ptr() };
                network::draw_network(&tctx.network, ctx, w, h, Some(da));
            }
        });
    }

    fn setup_term(&self) {
        let term = &self.imp().term_main;
        term.feed(b"Nadi Terminal: Run nadi tasks here.");
        term_prompt(term);
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
                // println!("{ch:?} {flag}");
                match ch {
                    "\r" => {
                        let line: &mut String =
                            unsafe { &mut *tm.data::<String>("current_line").unwrap().as_ptr() };
                        match line.trim() {
                            "" => (),
                            "clear" => tm.reset(true, false),
                            l => {
                                tm.feed(b"\r\x1B[A");
				term_prompt(&tm);
                                run_task(tm, &da, format!("{}\n", l));
                                // since the task could have changed the network properties
                                da.queue_draw();
                            }
                        };
                        line.clear();
                        term_prompt(&tm);
                    }
                    // Ctrl+C
                    "\u{3}" => {
                        let line: &mut String =
                            unsafe { &mut *tm.data::<String>("current_line").unwrap().as_ptr() };
                        line.clear();
                        tm.feed(b" ^C");
                        term_prompt(&tm);
                    }
                    // backspace
                    "\u{8}" => {
                        let line: &mut String =
                            unsafe { &mut *tm.data::<String>("current_line").unwrap().as_ptr() };
                        if line.pop().is_some() {
                            tm.feed(ch.as_bytes());
                        }
                    }
                    // tab
                    "\u{9}" => {
                        let line: &mut String =
                            unsafe { &mut *tm.data::<String>("current_line").unwrap().as_ptr() };
                        let cmd: Vec<String> =
                            line.trim().split(' ').map(|s| s.to_string()).collect();
                        match cmd[0].as_str() {
                            "node" => {
                                let f = nadi_functions(&da);
                                let funcs: Vec<&str> = f
                                    .node_alias()
                                    .keys()
                                    .chain(f.node_functions().keys())
                                    .map(|k| k.as_str())
                                    .collect();
                                let rest = cmd.get(1).map(|s| s.as_str()).unwrap_or_default();
                                completion(tm, line, rest, &funcs);
                            }
                            "network" => {
                                let f = nadi_functions(&da);
                                let funcs: Vec<&str> = f
                                    .network_alias()
                                    .keys()
                                    .chain(f.network_functions().keys())
                                    .map(|k| k.as_str())
                                    .collect();
                                let rest = cmd.get(1).map(|s| s.as_str()).unwrap_or_default();
                                completion(tm, line, rest, &funcs);
                            }
                            "help" => {
                                let f = nadi_functions(&da);
                                match cmd.get(1).map(|s| s.as_str()).unwrap_or_default() {
                                    "node" => {
                                        let funcs: Vec<&str> = f
                                            .node_alias()
                                            .keys()
                                            .chain(f.node_functions().keys())
                                            .map(|k| k.as_str())
                                            .collect();
                                        let rest =
                                            cmd.get(2).map(|s| s.as_str()).unwrap_or_default();
                                        completion(tm, line, rest, &funcs);
                                    }
                                    "network" => {
                                        let funcs: Vec<&str> = f
                                            .network_alias()
                                            .keys()
                                            .chain(f.network_functions().keys())
                                            .map(|k| k.as_str())
                                            .collect();
                                        let rest =
                                            cmd.get(2).map(|s| s.as_str()).unwrap_or_default();
                                        completion(tm, line, rest, &funcs);
                                    }
                                    rest => {
                                        let mut funcs: Vec<&str> = vec!["node", "network"];
                                        funcs.extend(f.node_alias().keys().map(|k| k.as_str()));
                                        funcs.extend(f.node_functions().keys().map(|k| k.as_str()));
                                        funcs.extend(f.network_alias().keys().map(|k| k.as_str()));
                                        funcs.extend(
                                            f.network_functions().keys().map(|k| k.as_str()),
                                        );
                                        completion(tm, line, rest, &funcs);
                                    }
                                }
                            }
                            x => completion(tm, line, x, &["node", "network", "help"]),
                        }
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

fn nadi_functions(darea: &gtk::DrawingArea) -> &NadiFunctions {
    if let Some(ctx) = unsafe { darea.data::<TaskContext>("tasks_ctx") } {
        let ctx: &TaskContext = unsafe { &*ctx.as_ptr() };
        &ctx.functions
    } else {
        panic!("Functions not found");
    }
}

fn run_task(term: &vte4::Terminal, darea: &gtk::DrawingArea, line: String) {
    if line.trim() == "help" {
	term.feed(b"TODO: Help \r\n");
        return;
    }
    match nadi_core::parser::tokenizer::get_tokens(&line) {
	Ok(tokens) => {
	    for t in &tokens {
		term.feed(t.colored().replace("\n", "\r\n").as_bytes());
	    }
	    term.feed("\r\n".as_bytes());
	    match nadi_core::parser::tasks::parse(tokens) {
		Ok(tasks) => {
		    run_tasks(term, darea, tasks);
		}
		Err(e) => {
		    term.feed(e.user_msg(None).replace("\n", "\r\n").as_bytes());
		}
	    }
	},
	Err(e) => {
	    term.feed(e.user_msg(None).replace("\n", "\r\n").as_bytes());
	}
    }
}


fn run_tasks(term: &vte4::Terminal, darea: &gtk::DrawingArea, tasks: Vec<nadi_core::tasks::Task>) {
    let mut skin = termimad::MadSkin::default_dark();
    for h in &mut skin.headers {
        h.align = termimad::Alignment::Left;
    }
    let tasks_ctx = if let Some(ctx) = unsafe { darea.data::<TaskContext>("tasks_ctx") } {
        let ctx: &mut TaskContext = unsafe { &mut *ctx.as_ptr() };
        ctx
    } else {
        term.feed(b"No Task Context Set; shouldn't happen; contact developers");
        return;
    };
    // temp solution, make NadiFunctions take a std::io::Write or
    // other trait object that can either print to stdout, or take the
    // result to show somewhere else (like here)
    let mut buf = gag::BufferRedirect::stdout().unwrap();
    let mut output = String::new();
    
    for fc in tasks {
        let res = tasks_ctx.execute(fc);
        // print the stdout output to the terminal
        buf.read_to_string(&mut output).unwrap();
        term.feed(output.replace("\n", "\r\n").as_bytes());
        output.clear();
	match res {
	    Ok(Some(p)) => term.feed(p.replace("\n", "\r\n").as_bytes()),
	    Err(p) => {
		term.feed(p.replace("\n", "\r\n").as_bytes());
		break;
	    },
	    _ => (),
	}
    }
}

fn push_func_help(skin: &termimad::MadSkin, term: &vte4::Terminal, signature: String, help: &str) {
    term.feed(signature.as_bytes());
    let txt = skin.text(help, Some(term.width() as usize));
    term.feed(txt.to_string().replace("\n", "\r\n").as_bytes());
    term.feed("\r\n".as_bytes());
}

fn term_prompt(tm: &vte4::Terminal) {
    tm.feed(format!("\r\n{} ", ">>".blue()).as_bytes())
}

fn completion(tm: &vte4::Terminal, line: &mut String, pre: &str, options: &[&str]) {
    let mut pos = options.iter().filter_map(|p| p.strip_prefix(pre));
    match pos.clone().count() {
        0 => tm.feed(&[7]), // bell
        1 => {
            let comp = pos.next().unwrap();
            line.push_str(comp);
            tm.feed(comp.as_bytes());
        }
        _ => {
            tm.feed(b"\r\n");
            tm.feed(
                pos.clone()
                    .map(|y| format!("{pre}{y}"))
                    .join(" ")
                    .as_bytes(),
            );
            term_prompt(tm);
            // add the common prefix from the alternatives
            let pos: Vec<&str> = pos.collect();
            let common = common_prefix(&pos);
            line.push_str(common);
            tm.feed(line.as_bytes());
        }
    }
}

fn common_prefix<'a>(strs: &'a [&str]) -> &'a str {
    if strs.is_empty() {
        return "";
    }
    let mut pre = strs[0].len();
    for s in strs.iter() {
        while !s.starts_with(&strs[0][0..pre]) {
            if pre <= 0 {
                return "";
            }
            pre -= 1; // Shorten the prefix
        }
    }
    &strs[0][0..pre]
}
