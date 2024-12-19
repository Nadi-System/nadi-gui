use abi_stable::std_types::{RSome, RString};
use cairo::Context;
use gdk::Rectangle;
use nadi_core::prelude::*;
use vte4::prelude::*;

// TODO make it better later

pub fn calc_hw(net: &Network, ctx: &Context) -> (i32, i32) {
    ctx.set_font_size(14.0);
    let offx = 10.0;
    let offy = 10.0;
    let dely = 20.0;
    let delx = 40.0;
    let mut left = offx;
    let max_lev = net
        .nodes()
        .map(|n| n.lock().level())
        .max()
        .unwrap_or_default();

    let mut text_start = left + delx * max_lev as f64 + offx;
    let max_text = net
        .nodes()
        .map(|n| {
            ctx.text_extents(&get_node_label(&n.lock()))
                .unwrap()
                .width()
        })
        .fold(0.0, f64::max);
    let w = (text_start + max_text).ceil() as i32;
    let req_height = (dely * (net.nodes_count() - 1) as f64) + offy * 2.0;
    let h = req_height.ceil() as i32;
    (h, w)
}

// works for cairo context in drawing area, not for svg export, the
// coordinates are off: needs more investigation
pub fn draw_network(
    net: &Network,
    ctx: &Context,
    w: i32,
    h: i32,
    darea: Option<&gtk::DrawingArea>,
) {
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
        .map(|n| {
            ctx.text_extents(&get_node_label(&n.lock()))
                .unwrap()
                .width()
        })
        .fold(0.0, f64::max);
    if (text_start + max_text) < w as f64 {
        left += (w as f64 - (text_start + max_text)) / 2.0;
        text_start += left - offx;
    } else if let Some(ref da) = darea {
        da.set_width_request((text_start + max_text).ceil() as i32);
    }
    let req_height = (dely * (net.nodes_count() - 1) as f64) + offy * 2.0;
    if req_height < h as f64 {
        top = (h / 2) as f64 + req_height / 2.0 - offy;
    } else if let Some(ref da) = darea {
        da.set_content_height(req_height.ceil() as i32);
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

fn get_node_color(node: &NodeInner) -> (f64, f64, f64) {
    node.try_attr::<(f64, f64, f64)>("nodecolor")
        .unwrap_or((0.0, 0.0, 0.0))
}

fn get_node_label(node: &NodeInner) -> String {
    let l = node
        .try_attr::<String>("nodelabel")
        .unwrap_or(node.name().to_string());
    if let Ok(templ) = nadi_core::string_template::Template::parse_template(&l) {
        if let Ok(text) = node.render(&templ) {
            return text;
        }
    }
    l
}

fn get_line_color(node: &NodeInner) -> (f64, f64, f64) {
    node.try_attr::<(f64, f64, f64)>("linecolor")
        .unwrap_or((0.0, 0.0, 0.0))
}

fn get_text_color(node: &NodeInner) -> (f64, f64, f64) {
    node.try_attr::<(f64, f64, f64)>("textcolor")
        .unwrap_or((0.0, 0.0, 0.0))
}
