mod imp;
use super::network;
use colored::Colorize;
use gio::ActionEntry;
use glib::{clone, Object};
use gtk::subclass::prelude::*;
use gtk::{gio, glib, Application, TextBuffer};
use gtk::{prelude::*, TextIter};
use itertools::Itertools;
use nadi_core::parser::tokenizer::{TaskToken, Token};
use nadi_core::parser::NadiError;
use nadi_core::{functions::NadiFunctions, network::Network, tasks::TaskContext};
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
            self.imp()
                .da_network
                .set_data("tasks_ctx", TaskContext::new(None));
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
            move |_| window.save_file().unwrap()
        ));

        self.imp().btn_export.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.export()
        ));

        // // The cursor position notify handles this as well
        // self.imp().tv_frame.connect_move_cursor(clone!(
        //     #[weak(rename_to=window)]
        //     self,
        //     move |tv, ms, step, _| {
        //         let buf = tv.buffer();
        //         let mut mark = buf.iter_at_mark(&buf.get_insert());
        //         // since this event seems to trigger before the cursor
        //         // is moved: simulating the movement
        //         match ms {
        //             gtk::MovementStep::DisplayLines => {
        //                 mark.forward_lines(step);
        //             }
        //             gtk::MovementStep::VisualPositions => {
        //                 mark.forward_chars(step);
        //             }
        //             gtk::MovementStep::Words => {
        //                 mark.forward_word_ends(step);
        //             }
        //             _ => (),
        //         }
        //         window.display_signature(mark);
        //     }
        // ));

        self.imp()
            .tv_frame
            .buffer()
            .connect_cursor_position_notify(clone!(
                #[weak(rename_to=window)]
                self,
                move |buf| {
                    // let buf = window.imp().tv_frame.buffer();
                    let mark = buf.iter_at_mark(&buf.get_insert());
                    window.display_signature(mark);
                }
            ));

        self.imp().tv_frame.connect_insert_at_cursor(move |tv, s| {
            println!("Inserted {s}");
            if s == "0" {
                tv.buffer().insert_at_cursor(s);
            }
        });

        self.imp().tv_frame.buffer().connect_changed(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| {
                let tb = window.imp().tv_frame.buffer();
                // todo, only do this for current line
                tb.remove_all_tags(&tb.start_iter(), &tb.end_iter());
                window.refresh_signature();
                window.format_task(&tb);
            }
        ));

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
                run_task(
                    tm,
                    &window.imp().da_network,
                    format!("{}\n", selection.trim()),
                );
                term_prompt(&tm);
                // since the task could have changed the network properties
                window.imp().da_network.queue_draw();
            }
        ));

        self.imp().btn_comment.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| {
                let buf = window.imp().tv_frame.buffer();
                let mut mark = buf.iter_at_mark(&buf.selection_bound());
                let mut ins = buf.iter_at_mark(&buf.get_insert());
                if mark == ins {
                    mark = buf.start_iter();
                    ins = buf.end_iter();
                }
                let selection = buf.text(&ins, &mark, true);
                let iscomment = selection
                    .lines()
                    .map(|l| l.trim())
                    .all(|l| l.is_empty() || l.starts_with('#'));
                let mut newlines = String::new();
                if iscomment {
                    for l in selection.lines() {
                        if !l.trim().is_empty() {
                            let (x, y) = l.split_once('#').expect("should have #");
                            newlines.push_str(x);
                            if y.starts_with(' ') {
                                newlines.push_str(&y[1..]);
                            } else {
                                newlines.push_str(y);
                            }
                        }
                        newlines.push('\n');
                    }
                } else {
                    for l in selection.lines() {
                        if !l.trim().is_empty() {
                            newlines.push('#');
                            newlines.push(' ');
                            newlines.push_str(l);
                        }
                        newlines.push('\n');
                    }
                }
                if !selection.ends_with('\n') {
                    // remove the extra '\n' is not in selection as
                    // lines() ignores the last '\n'
                    newlines.pop();
                }
                buf.delete(&mut mark, &mut ins);
                buf.insert(&mut mark, &newlines);
                let mut prev = mark;
                prev.backward_chars(newlines.chars().count() as i32);
                buf.select_range(&prev, &mark);
                window.refresh_signature();
            }
        ));
    }

    fn refresh_signature(&self) {
        let buf = self.imp().tv_frame.buffer();
        let mark = buf.iter_at_mark(&buf.get_insert());
        self.display_signature(mark);
    }

    fn display_signature(&self, mark: TextIter) {
        let buf = self.imp().tv_frame.buffer();
        let line = buf
            .text(&buf.start_iter(), &mark, false)
            .split('\n')
            .count()
            - 1;
        let mut end = buf.iter_at_line(line as i32).expect("should be valid line");
        let start = end;
        end.forward_line();
        let line = buf.text(&start, &end, false);
        match nadi_core::parser::tokenizer::get_tokens(&line) {
            Ok(tags) => {
                if let Some(t) = tags
                    .into_iter()
                    .filter(|t| t.ty == TaskToken::Function)
                    .next()
                {
                    let tasks_ctx = if let Some(ctx) =
                        unsafe { self.imp().da_network.data::<TaskContext>("tasks_ctx") }
                    {
                        let ctx: &mut TaskContext = unsafe { &mut *ctx.as_ptr() };
                        ctx
                    } else {
                        return;
                    };
                    let func = if line.trim().starts_with("node") {
                        tasks_ctx
                            .functions
                            .node(&t.content)
                            .map(|f| (f.signature(), f.short_help()))
                    } else if line.trim().starts_with("net") {
                        tasks_ctx
                            .functions
                            .network(&t.content)
                            .map(|f| (f.signature(), f.short_help()))
                    } else {
                        tasks_ctx
                            .functions
                            .network(&t.content)
                            .map(|f| (f.signature(), f.short_help()))
                            .or_else(|| {
                                tasks_ctx
                                    .functions
                                    .node(&t.content)
                                    .map(|f| (f.signature(), f.short_help()))
                            })
                    };
                    if let Some((sig, help)) = func {
                        let sig = format!("<span foreground=\"purple\">{}</span><span foreground=\"gray\">{}</span>\n<span foreground=\"yellow\" size=\"small\">{}</span>", &t.content, sig.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;"), help);
                        self.imp().lab_signature.set_markup(&sig);
                    }
                }
            }
            Err(_) => (),
        }
    }

    fn format_task(&self, tb: &gtk::TextBuffer) {
        let mut point = tb.start_iter();
        apply_tags(&mut point, tb)
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
        self.refresh_signature();
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
        let tasks_ctx = TaskContext::new(None);
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
        if self.imp().btn_sync.is_active() {
            self.reload_network()?
        }
        Ok(())
    }

    pub fn open_file(&self, file: &gtk::gio::File) -> anyhow::Result<()> {
        let filename = file.path().expect("Couldn't get file path");
        let name = filename.to_string_lossy().to_string();
        self.imp().txt_browse.set_text(&name);
        let txt = std::fs::read_to_string(&name)?;
        self.imp().tv_frame.buffer().set_text(&txt);
        self.refresh_signature();
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
                    let mut png =
                        cairo::ImageSurface::create(cairo::Format::ARgb32, w * 10, h * 10).unwrap();
                    let ctx = cairo::Context::new(&mut png).unwrap();
                    ctx.scale(10.0, 10.0);
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
        }
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
            }
            _ => (),
        }
    }
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

fn apply_tags(point: &mut TextIter, tb: &TextBuffer) {
    let text = tb.text(&point, &tb.end_iter(), true);
    match nadi_core::parser::tokenizer::get_tokens(&text) {
        Ok(tags) => apply_token_tags(point, tb, &tags),
        Err(e) => {
            // there is an error somewhere; so we skip that line
            let valid = text.split("\n").take(e.line).join("\n");
            match nadi_core::parser::tokenizer::get_tokens(&valid) {
                Ok(tags) => apply_token_tags(point, tb, &tags),
                Err(_e) => {
                    // This should never happen, but it happens when
                    // there is problem with strings ""
                    // println!("{}", e.user_msg(None));
                    // panic!("Should have been valid");
                    return;
                }
            }
            let l = *point;
            point.forward_line();
            tb.apply_tag_by_name("error", &l, &point);
            apply_tags(point, tb);
        }
    }
}

fn apply_token_tags(point: &mut TextIter, tb: &TextBuffer, tokens: &[Token]) {
    for t in tokens {
        let st = *point;
        point.forward_chars(t.content.chars().count() as i32);
        let tg = match t.ty {
            TaskToken::Comment => "comment",
            TaskToken::Keyword(_) => "keyword",
            TaskToken::Function => "function",
            TaskToken::Variable => "variable",
            TaskToken::Bool => "bool",
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
}
