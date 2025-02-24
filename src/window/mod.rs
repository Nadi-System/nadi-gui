mod imp;
use super::network;
use crate::tokens::TokenMarkup;
use gio::ActionEntry;
use glib::{clone, Object};
use gtk::subclass::prelude::*;
use gtk::{gio, glib, Application, TextBuffer};
use gtk::{prelude::*, TextIter};
use itertools::Itertools;
use nadi_core::parser::tokenizer::{self, TaskToken, Token};
use nadi_core::parser::{NadiError, ParseError, ParseErrorType};
use nadi_core::tasks::TaskKeyword;
use nadi_core::{
    functions::FuncArgType,
    network::Network,
    tasks::{Task, TaskContext},
};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::iter::Iterator;

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
        let ctx = TaskContext::new(None);
        unsafe {
            self.imp().da_network.set_data("tasks_ctx", ctx);
        }
    }

    #[allow(deprecated)]
    fn setup_menu(&self) {
        let ctx =
            if let Some(ctx) = unsafe { self.imp().da_network.data::<TaskContext>("tasks_ctx") } {
                let ctx: &mut TaskContext = unsafe { &mut *ctx.as_ptr() };
                ctx
            } else {
                return;
            };

        let funcs = &self.imp().menu_functions;
        let env = gio::Menu::new();
        let node = gio::Menu::new();
        let network = gio::Menu::new();
        let mut action_entries = vec![];
        let functions = ctx
            .functions
            .env_functions()
            .keys()
            .sorted()
            .map(|f| ("env", &env, f))
            .chain(
                ctx.functions
                    .node_functions()
                    .keys()
                    .sorted()
                    .map(|f| ("node", &node, f)),
            )
            .chain(
                ctx.functions
                    .network_functions()
                    .keys()
                    .sorted()
                    .map(|f| ("network", &network, f)),
            );

        // command entry
        let compl = gtk::EntryCompletion::new();
        // Use the first (and only) column available to set the autocompletion text
        compl.set_text_column(0);
        compl.set_minimum_key_length(1);
        compl.set_popup_completion(true);
        // Create a ListStore of items
        let store = gtk::ListStore::new(&[glib::Type::STRING]);
        let plugins = gio::Menu::new();
        let mut plugins_each = HashMap::new();
        for (t, m, n) in functions {
            let act = n.as_str();
            let name = n.to_string();
            store.set(&store.append(), &[(0, &format!("{t} {act}"))]);
            action_entries.push({
                ActionEntry::builder(&format!("{t}.{act}"))
                    .activate(move |window: &Window, _, _| {
                        let tv = &window.imp().tv_frame;
                        tv.grab_focus();
                        let pre = match t {
                            "node" | "network" => format!("{t} "),
                            _ => String::new(),
                        };
                        let buf = tv.buffer();
                        buf.insert_at_cursor(&format!("{pre}{name}()"));
                        let mut ins = buf.iter_at_mark(&buf.get_insert());
                        ins.backward_char();
                        buf.place_cursor(&ins);
                        tv.grab_focus();
                    })
                    .build()
            });
            m.append_item(&gio::MenuItem::new(
                Some(&n.replace("_", "-")),
                Some(&format!("win.{t}.{act}")),
            ));
            let (plugin, func) = n
                .split_once('.')
                .expect("Function name should be plugin.name");
            store.set(&store.append(), &[(0, &format!("{t} {func}"))]);
            let pm = plugins_each.entry(plugin).or_insert(gio::Menu::new());
            pm.append_item(&gio::MenuItem::new(
                Some(&format!("{t} {}", func.replace("_", "-"))),
                Some(&format!("win.{t}.{act}")),
            ));
        }
        compl.set_model(Some(&store));
        self.imp().txt_term.set_completion(Some(&compl));
        funcs.append_submenu(Some("Environment"), &env);
        funcs.append_submenu(Some("Node"), &node);
        funcs.append_submenu(Some("Network"), &network);
        for n in plugins_each.keys().sorted() {
            plugins.append_submenu(Some(&n.replace("_", "-")), &plugins_each[n]);
        }
        funcs.append_section(Some("Plugins"), &plugins);
        self.add_action_entries(action_entries);
    }

    fn setup_actions(&self) {
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
        let action_new = ActionEntry::builder("new")
            .activate(|window: &Window, _, _| {
                let _ = window.new_file();
            })
            .build();
        let action_save = ActionEntry::builder("save")
            .activate(|window: &Window, _, _| {
                let _ = window.save_file();
            })
            .build();
        let action_save_as = ActionEntry::builder("saveas")
            .activate(|window: &Window, _, _| {
                let _ = window.save_file_as();
            })
            .build();
        let action_refresh = ActionEntry::builder("refresh")
            .activate(|window: &Window, _, _| {
                window.imp().da_network.queue_draw();
            })
            .build();
        let action_export = ActionEntry::builder("export")
            .activate(|window: &Window, _, _| {
                window.export();
            })
            .build();
        let action_run_func = ActionEntry::builder("run_func")
            .activate(|window: &Window, _, _| {
                window.run_func();
            })
            .build();
        let action_run_line = ActionEntry::builder("run_line")
            .activate(|window: &Window, _, _| {
                window.run_line();
            })
            .build();
        let action_run_buffer = ActionEntry::builder("run_buffer")
            .activate(|window: &Window, _, _| {
                window.run_buffer();
            })
            .build();
        let action_run_term = ActionEntry::builder("run_term")
            .activate(|window: &Window, _, _| {
                window.run_term();
            })
            .build();
        let action_help = ActionEntry::builder("help_line")
            .activate(|window: &Window, _, _| {
                window.help_line();
            })
            .build();
        let action_comment = ActionEntry::builder("toggle_comment")
            .activate(|window: &Window, _, _| {
                window.toggle_comment();
            })
            .build();
        let action_book = ActionEntry::builder("book")
            .activate(|window: &Window, _, _| {
                window.book();
            })
            .build();
        let action_about = ActionEntry::builder("about")
            .activate(|window: &Window, _, _| {
                window.about();
            })
            .build();
        self.add_action_entries([
            action_open,
            action_close,
            action_new,
            action_save,
            action_save_as,
            action_refresh,
            action_export,
            action_run_func,
            action_run_line,
            action_run_buffer,
            action_run_term,
            action_help,
            action_comment,
            action_book,
            action_about,
        ]);
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

        self.imp().btn_sig.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.help_line()
        ));

        self.imp()
            .tv_frame
            .buffer()
            .connect_cursor_position_notify(clone!(
                #[weak(rename_to=window)]
                self,
                move |buf| {
                    // let buf = window.imp().tv_frame.buffer();
                    let mark = buf.iter_at_mark(&buf.get_insert());
                    window.display_signature_editor(mark);
                }
            ));

        // self.imp().tv_frame.connect_insert_at_cursor(move |tv, s| {
        //     println!("Inserted {s}");
        //     if s == "0" {
        //         tv.buffer().insert_at_cursor(s);
        //     }
        // });

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

        self.imp().btn_run_func.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.run_func()
        ));

        self.imp().btn_run_line.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.run_line()
        ));

        self.imp().btn_run_buffer.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.run_buffer()
        ));

        self.imp().btn_term_top.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.term_scroll_to_top()
        ));
        self.imp().btn_term_up.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.term_scroll_up()
        ));

        self.imp().btn_term_down.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.term_scroll_down()
        ));
        self.imp().btn_term_bottom.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.term_scroll_to_end()
        ));

        self.imp().txt_term.connect_activate(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.run_term()
        ));

        self.imp().txt_term.connect_changed(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.display_signature_term()
        ));

        self.imp().tv_term.buffer().connect_changed(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.term_scroll_to_end()
        ));

        self.imp().btn_comment.connect_clicked(clone!(
            #[weak(rename_to=window)]
            self,
            move |_| window.toggle_comment()
        ));
    }

    fn toggle_comment(&self) {
        let buf = self.imp().tv_frame.buffer();
        let mut mark = buf.iter_at_mark(&buf.selection_bound());
        let mut ins = buf.iter_at_mark(&buf.get_insert());
        let is_selection = mark != ins;
        if !is_selection {
            mark = buf.iter_at_line(ins.line()).unwrap();
            if !ins.ends_line() {
                ins.forward_to_line_end();
            }
        };
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
        if is_selection {
            let mut prev = mark;
            prev.backward_chars(newlines.chars().count() as i32);
            buf.select_range(&prev, &mark);
        }
        self.refresh_signature();
    }

    fn task_betwn_marks(&self, s: &TextIter, e: &TextIter) -> Option<Vec<Task>> {
        let buf = self.imp().tv_frame.buffer();
        let text = buf.text(s, e, true);
        let tokens = tokenizer::get_tokens(&text).ok()?;
        nadi_core::parser::tasks::parse(tokens).ok()
    }

    fn task_at_mark(&self) -> (TextIter, String, Vec<Task>) {
        let buf = self.imp().tv_frame.buffer();
        let mut ins = buf.iter_at_mark(&buf.get_insert());
        let mut line = ins.line();
        let mut mark;
        let mut tasks = vec![];
        loop {
            // seek backwards until we find a task keyword to find the start
            if line < 0 {
                mark = buf.start_iter();
                break;
            }
            mark = buf.iter_at_line(line).unwrap();
            let ins2 = buf.iter_at_line(line + 1).unwrap();
            let text = buf.text(&mark, &ins2, true);
            let task_tokens = match tokenizer::get_tokens(&text) {
                Ok(t) => t,
                Err(_) => {
                    line -= 1;
                    continue;
                }
            };
            let tokens = tokenizer::VecTokens::new(task_tokens);
            let start_ok = match tokens.peek_next_no_ws(true) {
                Some(t) => match t.ty {
                    TaskToken::Keyword(TaskKeyword::In)
                    | TaskToken::Keyword(TaskKeyword::Match) => false,
                    TaskToken::Keyword(_) => true,
                    _ => false,
                },
                None => true,
            };
            if !start_ok {
                line -= 1;
            } else {
                break;
            }
        }
        let mut text = String::new();
        let mut line = ins.line();
        loop {
            // seek forward until we have a complete task
            line += 1;
            ins = match buf.iter_at_line(line) {
                Some(i) => i,
                None => {
                    ins = buf.end_iter();
                    break;
                }
            };
            text = buf.text(&mark, &ins, true).trim().to_string();
            let task_tokens = match tokenizer::get_tokens(&text) {
                Ok(t) => t,
                Err(_) => continue,
            };
            if task_tokens.iter().any(|t| t.ty == TaskToken::Quote) {
                // there is unclosed string there
                ()
            } else {
                match nadi_core::parser::tasks::parse(task_tokens) {
                    Ok(v) => {
                        if !v.is_empty() {
                            tasks = v;
                            break;
                        }
                    }
                    Err(e) => match e.ty {
                        nadi_core::parser::ParseErrorType::Unclosed => (),
                        _ => break,
                    },
                }
            };
        }
        (ins, text, tasks)
    }

    fn run_func(&self) {
        let buf = self.imp().tv_frame.buffer();
        let (ins, tokens, tasks) = self.task_at_mark();
        self.feed_term_tasks(&tokens);
        self.run_tasks(tasks);
        buf.place_cursor(&ins);
        self.imp()
            .tv_frame
            .scroll_to_mark(&buf.get_insert(), 0.1, false, 0.0, 0.0);
        self.imp().tv_frame.grab_focus();
    }

    fn run_line(&self) {
        let buf = self.imp().tv_frame.buffer();
        let mut mark = buf.iter_at_mark(&buf.selection_bound());
        let mut ins = buf.iter_at_mark(&buf.get_insert());
        if mark == ins {
            mark = buf.iter_at_line(ins.line()).unwrap();
            if !ins.ends_line() {
                ins.forward_to_line_end();
            }
        };
        self.feed_term_tasks(&buf.text(&mark, &ins, true));
        if let Some(t) = self.task_betwn_marks(&mark, &ins) {
            self.run_tasks(t);
        }
    }

    fn run_buffer(&self) {
        let buf = self.imp().tv_frame.buffer();
        let mark = buf.start_iter();
        let ins = buf.end_iter();
        self.feed_term_tasks(&buf.text(&mark, &ins, true));
        if let Some(t) = self.task_betwn_marks(&mark, &ins) {
            self.run_tasks(t);
        }
    }

    fn run_term(&self) {
        let text = self.imp().txt_term.text();
        self.feed_term_tasks(&text);
        match tokenizer::get_tokens(&text) {
            Ok(tk) => match nadi_core::parser::tasks::parse(tk) {
                Ok(t) => {
                    self.run_tasks(t);
                    self.imp().txt_term.set_text("");
                }
                Err(e) => {
                    self.term_parse_err(e);
                }
            },
            Err(e) => {
                self.feed_term_markup(&e.user_msg(None));
            }
        }
    }

    fn run_tasks(&self, tasks: Vec<Task>) {
        let term = &self.imp().tv_term;
        let tasks_ctx =
            if let Some(ctx) = unsafe { self.imp().da_network.data::<TaskContext>("tasks_ctx") } {
                let ctx: &mut TaskContext = unsafe { &mut *ctx.as_ptr() };
                ctx
            } else {
                term.buffer()
                    .insert_at_cursor("No Task Context Set; shouldn't happen; contact developers");
                return;
            };
        // temp solution, make NadiFunctions take a std::io::Write or
        // other trait object that can either print to stdout, or take the
        // result to show somewhere else (like here)
        let mut buf = gag::BufferRedirect::stdout().unwrap();
        let mut output = String::new();

        let total = tasks.len();
        for (i, fc) in tasks.into_iter().enumerate() {
            let res = tasks_ctx.execute(fc);
            // print the stdout output to the terminal
            buf.read_to_string(&mut output).unwrap();
            self.feed_term_text(&output);
            output.clear();
            match res {
                Ok(Some(p)) => {
                    self.feed_term_text(&p);
                    self.feed_term_text("\n");
                }
                Err(p) => {
                    self.feed_term_text(&p);
                    self.feed_term_text("\n");
                    break;
                }
                _ => (),
            }
            self.imp()
                .pb_term
                .set_fraction((i + 1) as f64 / total as f64);
            self.imp()
                .pb_term
                .set_tooltip_text(Some(&format!("{} of {}", i + 1, total)));
        }
        self.term_prompt();
        // since the task could have changed the network properties
        self.imp().da_network.queue_draw();
    }

    fn refresh_signature(&self) {
        let buf = self.imp().tv_frame.buffer();
        let mark = buf.iter_at_mark(&buf.get_insert());
        self.display_signature_editor(mark);
    }

    fn help_line(&self) {
        self.feed_term_markup(&self.imp().btn_sig.tooltip_markup().unwrap_or_default());
    }

    fn display_signature_editor(&self, mark: TextIter) {
        let buf = self.imp().tv_frame.buffer();
        let mut end = buf.iter_at_line(mark.line()).expect("should be valid line");
        let start = end;
        end.forward_line();
        let line = buf.text(&start, &end, false);
        self.display_signature(&line);
    }

    fn display_signature_term(&self) {
        let line = self.imp().txt_term.text();
        self.display_signature(&line);
    }

    fn display_signature(&self, line: &str) {
        match tokenizer::get_tokens(&line) {
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
                            .map(|f| (f.args(), f.short_help(), f.help()))
                            .or_else(|| {
                                tasks_ctx
                                    .functions
                                    .env(&t.content)
                                    .map(|f| (f.args(), f.short_help(), f.help()))
                            })
                    } else if line.trim().starts_with("net") {
                        tasks_ctx
                            .functions
                            .network(&t.content)
                            .map(|f| (f.args(), f.short_help(), f.help()))
                            .or_else(|| {
                                tasks_ctx
                                    .functions
                                    .env(&t.content)
                                    .map(|f| (f.args(), f.short_help(), f.help()))
                            })
                    } else {
                        tasks_ctx
                            .functions
                            .env(&t.content)
                            .map(|f| (f.args(), f.short_help(), f.help()))
                            .or_else(|| {
                                tasks_ctx
                                    .functions
                                    .node(&t.content)
                                    .map(|f| (f.args(), f.short_help(), f.help()))
                                    .or_else(|| {
                                        tasks_ctx
                                            .functions
                                            .network(&t.content)
                                            .map(|f| (f.args(), f.short_help(), f.help()))
                                    })
                            })
                    };
                    if let Some((args, help, fullhelp)) = func {
                        let args_color: Vec<String> = args
                            .iter()
                            .map(|f| {
                                let (n, t, v) = match &f.category {
                                    FuncArgType::Arg => (
                                        format!("<span foreground=\"limegreen\">{}</span>", f.name),
                                        &f.ty,
                                        "".into(),
                                    ),
                                    FuncArgType::OptArg => (
                                        format!("<span foreground=\"green\">{}</span>", f.name),
                                        &f.ty,
                                        "".into(),
                                    ),
                                    FuncArgType::DefArg(val) => (
                                        format!("<span foreground=\"green\">{}</span>", f.name),
                                        &f.ty,
                                        format!(" = <span foreground=\"yellow\">{}</span>", val),
                                    ),
                                    FuncArgType::Args => {
                                        return format!(
                                            "<span foreground=\"red\">*{}</span>",
                                            f.name
                                        )
                                    }
                                    FuncArgType::KwArgs => {
                                        return format!(
                                            "<span foreground=\"red\">**{}</span>",
                                            f.name
                                        )
                                    }
                                };
                                format!(
                                    "{}: <span foreground=\"gray\">{}</span>{}",
                                    n,
                                    t.replace("&", "&amp;")
                                        .replace("<", "&lt;")
                                        .replace(">", "&gt;"),
                                    v
                                )
                            })
                            .collect();
                        let mut sig = format!("<span size=\"small\"><span foreground=\"purple\">{}</span>({})</span>\n<span foreground=\"gray\" size=\"small\">{}</span>", &t.content, args_color.join(", "), help);
                        self.imp().lab_signature.set_markup(&sig);
                        sig.push_str("\n<span size=\"small\"><b>Arguments:</b>\n");
                        for (a, ac) in args.into_iter().zip(args_color) {
                            sig.push_str(&format!("- {} : {}\n", ac, a.help));
                        }
                        sig.push_str("</span>\n<span size=\"small\"><b>Description:</b>\n");
                        sig.push_str(fullhelp[help.len()..].trim());
                        sig.push_str("</span>\n");
                        self.imp().btn_sig.set_tooltip_markup(Some(&sig));
                    } else {
                        let sig = format!("<span size=\"small\"><span foreground=\"purple\">{}</span>()</span>\n<span foreground=\"red\" size=\"small\">Function Does Not Exist. Please Make sure you spelled it correct, or loaded all plugins.</span>", &t.content);
                        self.imp().lab_signature.set_markup(&sig);
                        self.imp().btn_sig.set_tooltip_markup(Some(&sig));
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

    pub fn book(&self) {
        let _ = webbrowser::open_browser(
            webbrowser::Browser::Default,
            "https://nadi-system.github.io/preface.html",
        );
    }

    pub fn about(&self) {
        let diag = gtk::AboutDialog::builder()
            .program_name("Network Analysis and Data Integration (NADI)")
            .version(format!(
                "{} (nadi_core: {})",
                env!("CARGO_PKG_VERSION"),
                nadi_core::NADI_CORE_VERSION
            ))
            .logo_icon_name("nadi")
            .website("https://nadi-system.github.io")
            .authors(["Gaurav Atreya <allmanpride@gmail.com>"])
            .license_type(gtk::License::Gpl30)
            .build();
        diag.present();
    }

    pub fn export(&self) {
        let filters = gtk::FileFilter::new();
        for mime in ["image/png", "image/svg", "application/pdf"] {
            filters.add_mime_type(mime);
        }
        let mut dialog = gtk::FileDialog::builder()
            .title("Export Image File")
            .default_filter(&filters)
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
        let tasks_ctx = TaskContext::new(None);
        unsafe {
            self.imp().da_network.set_data("tasks_ctx", tasks_ctx);
        }
        self.run_buffer();
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

    pub fn new_file(&self) -> anyhow::Result<()> {
        self.browse_new_file(|w| {
            let buf = w.imp().tv_frame.buffer();
            w.imp()
                .tv_frame
                .buffer()
                .delete(&mut buf.start_iter(), &mut buf.end_iter());
        });
        Ok(())
    }

    pub fn browse_new_file(&self, callback: fn(Window)) {
        let filters = gtk::FileFilter::new();
        filters.add_pattern("*.tasks");
        let mut dialog = gtk::FileDialog::builder()
            .title("Save Tasks File As")
            .default_filter(&filters)
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
                        let filename = file.path().expect("Couldn't get file path");
                        let name = filename.to_string_lossy().to_string();
                        window.imp().txt_browse.set_text(&name);
                        callback(window);
                    }
                }
            ),
        );
    }

    pub fn save_file_as(&self) {
        self.browse_new_file(|w| {
            if let Err(e) = w.save_file() {
                eprintln!("Error saving file: {e:?}");
            }
        });
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
        let term = &self.imp().tv_term;
        term.buffer()
            .insert_at_cursor("Nadi Terminal: Run nadi tasks here.");
        self.term_prompt();
        unsafe { term.set_data("history", Vec::<String>::new()) };
        term.buffer()
            .create_mark(Some("endmark"), &term.buffer().end_iter(), false);
    }

    // this is also used by the commands instead of just the button,
    // so I'm trying my best to make it work any time, but I think it
    // can't work without taking it out and delaying this activation
    // to let the textview can calculate/render, it'd work well
    fn term_scroll_to_end(&self) {
        let mark = self
            .imp()
            .tv_term
            .buffer()
            .mark("endmark")
            .unwrap_or_else(|| {
                let buf = self.imp().tv_term.buffer();
                buf.create_mark(Some("endmark"), &buf.end_iter(), false)
            });
        self.imp().tv_term.scroll_mark_onscreen(&mark);
    }
    fn term_scroll_to_top(&self) {
        self.imp().tv_term.scroll_to_iter(
            &mut self.imp().tv_term.buffer().start_iter(),
            0.0,
            false,
            0.0,
            0.0,
        );
    }
    fn term_scroll_up(&self) {
        let buf = self.imp().tv_term.buffer();
        let mut ins = buf.iter_at_mark(&buf.get_insert());
        ins.backward_lines(5);
        self.imp()
            .tv_term
            .scroll_to_iter(&mut ins, 0.0, false, 0.0, 0.0);
        buf.place_cursor(&ins);
    }
    fn term_scroll_down(&self) {
        let buf = self.imp().tv_term.buffer();
        let mut ins = buf.iter_at_mark(&buf.get_insert());
        ins.forward_lines(5);
        self.imp()
            .tv_term
            .scroll_to_iter(&mut ins, 0.0, false, 0.0, 0.0);
        buf.place_cursor(&ins);
    }

    fn term_prompt(&self) {
        self.feed_term_markup("\n<span fgcolor=\"blue\">&gt;&gt;</span> ")
    }

    fn term_parse_err(&self, err: ParseError) {
        self.feed_term_markup("\n<span fgcolor=\"red\">Error</span>: ");
        self.feed_term_markup(&format!(
            "ParseError at Line {} Column {}\n",
            err.line, err.col
        ));
        self.feed_term_markup(&format!("  {}\n", err.linestr));
        self.feed_term_markup(&format!(
            "  {: >2$} <span fgcolor=\"yellow\">{}</span>",
            "^",
            err.ty.message(),
            err.col + 1
        ));
        if let ParseErrorType::LogicalError(s) = &err.ty {
            self.feed_term_markup(&format!("\n  <span fgcolor=\"red\">{}</span>", s));
        }
        self.term_prompt();
    }

    fn feed_term_text(&self, txt: &str) {
        let buf = self.imp().tv_term.buffer();
        buf.insert(&mut buf.end_iter(), txt);
    }

    fn feed_term_markup(&self, txt: &str) {
        let buf = self.imp().tv_term.buffer();
        buf.insert_markup(&mut buf.end_iter(), txt);
    }

    fn feed_term_tasks(&self, tasks: &str) {
        let tokens = match tokenizer::get_tokens(&tasks) {
            Ok(t) => t,
            Err(_) => return,
        };
        let buf = self.imp().tv_term.buffer();
        let markup = tokens.iter().map(|t| t.markup()).join("");
        buf.insert_markup(&mut buf.end_iter(), &markup);
        buf.insert(&mut buf.end_iter(), "\n");
        // for line in markup.split("<br>") {
        //     buf.insert_markup(&mut buf.end_iter(), line);
        //     println!("{line}");
        //     buf.insert(&mut buf.end_iter(), "\n");
        // }
    }
}

// fn nadi_functions(darea: &gtk::DrawingArea) -> &NadiFunctions {
//     if let Some(ctx) = unsafe { darea.data::<TaskContext>("tasks_ctx") } {
//         let ctx: &TaskContext = unsafe { &*ctx.as_ptr() };
//         &ctx.functions
//     } else {
//         panic!("Functions not found");
//     }
// }

// fn completion(tm: &gtk::TextView, line: &mut String, pre: &str, options: &[&str]) {
//     let mut pos = options.iter().filter_map(|p| p.strip_prefix(pre));
//     match pos.clone().count() {
//         0 => tm.feed(&[7]), // bell
//         1 => {
//             let comp = pos.next().unwrap();
//             line.push_str(comp);
//             tm.feed(comp.as_bytes());
//         }
//         _ => {
//             tm.feed(b"\r\n");
//             tm.feed(
//                 pos.clone()
//                     .map(|y| format!("{pre}{y}"))
//                     .join(" ")
//                     .as_bytes(),
//             );
//             term_prompt(tm);
//             // add the common prefix from the alternatives
//             let pos: Vec<&str> = pos.collect();
//             let common = common_prefix(&pos);
//             line.push_str(common);
//             tm.feed(line.as_bytes());
//         }
//     }
// }

// fn common_prefix<'a>(strs: &'a [&str]) -> &'a str {
//     if strs.is_empty() {
//         return "";
//     }
//     let mut pre = strs[0].len();
//     for s in strs.iter() {
//         while !s.starts_with(&strs[0][0..pre]) {
//             if pre <= 0 {
//                 return "";
//             }
//             pre -= 1; // Shorten the prefix
//         }
//     }
//     &strs[0][0..pre]
// }

fn apply_tags(point: &mut TextIter, tb: &TextBuffer) {
    let text = tb.text(&point, &tb.end_iter(), true);
    match tokenizer::get_tokens(&text) {
        Ok(tags) => apply_token_tags(point, tb, &tags),
        Err(e) => {
            // there is an error somewhere; so we skip that line
            let valid = text.split("\n").take(e.line).join("\n");
            match tokenizer::get_tokens(&valid) {
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
            TaskToken::Quote => "error2",
            _ => continue,
        };
        tb.apply_tag_by_name(tg, &st, &point);
    }
}
