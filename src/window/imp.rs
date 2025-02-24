use glib::subclass::InitializingObject;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

// Object holding the state
#[derive(CompositeTemplate, Default)]
#[template(resource = "/org/zerosofts/NadiGui/window.ui")]
pub struct Window {
    #[template_child]
    pub main_menubar: TemplateChild<gtk::PopoverMenuBar>,
    #[template_child]
    pub menu_functions: TemplateChild<gtk::gio::Menu>,
    #[template_child]
    pub txt_browse: TemplateChild<gtk::Text>,
    #[template_child]
    pub btn_browse: TemplateChild<gtk::Button>,
    #[template_child]
    pub btn_save: TemplateChild<gtk::Button>,
    #[template_child]
    pub da_network: TemplateChild<gtk::DrawingArea>,
    #[template_child]
    pub btn_sig: TemplateChild<gtk::Button>,
    #[template_child]
    pub lab_signature: TemplateChild<gtk::Label>,
    #[template_child]
    pub tv_frame: TemplateChild<gtk::TextView>,
    #[template_child]
    pub btn_sync: TemplateChild<gtk::ToggleButton>,
    #[template_child]
    pub btn_run_func: TemplateChild<gtk::Button>,
    #[template_child]
    pub btn_run_line: TemplateChild<gtk::Button>,
    #[template_child]
    pub btn_run_buffer: TemplateChild<gtk::Button>,
    #[template_child]
    pub btn_comment: TemplateChild<gtk::Button>,
    #[template_child]
    pub btn_export: TemplateChild<gtk::Button>,
    #[template_child]
    pub btn_term_top: TemplateChild<gtk::Button>,
    #[template_child]
    pub btn_term_up: TemplateChild<gtk::Button>,
    #[template_child]
    pub btn_term_down: TemplateChild<gtk::Button>,
    #[template_child]
    pub btn_term_bottom: TemplateChild<gtk::Button>,
    #[template_child]
    pub txt_term: TemplateChild<gtk::Entry>,
    #[template_child]
    pub pb_term: TemplateChild<gtk::ProgressBar>,
    #[template_child]
    pub tv_term: TemplateChild<gtk::TextView>,
}

// The central trait for subclassing a GObject
#[glib::object_subclass]
impl ObjectSubclass for Window {
    // `NAME` needs to match `class` attribute of template
    const NAME: &'static str = "NadiGuiWindow";
    type Type = super::Window;
    type ParentType = gtk::ApplicationWindow;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &InitializingObject<Self>) {
        obj.init_template();
    }
}

// Trait shared by all GObjects
impl ObjectImpl for Window {
    fn constructed(&self) {
        // Call "constructed" on parent
        self.parent_constructed();
        // Setup
        let obj = self.obj();
        obj.setup_data();
        obj.setup_callbacks();
        obj.setup_actions();
        obj.setup_drawing_area();
        obj.setup_term();
        obj.setup_menu();
    }
}

// Trait shared by all widgets
impl WidgetImpl for Window {}

// Trait shared by all windows
impl WindowImpl for Window {}

// Trait shared by all application windows
impl ApplicationWindowImpl for Window {}
