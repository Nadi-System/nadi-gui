use super::colors::AttrColor;
use abi_stable::std_types::RSome;
use cairo::Context;
use nadi_core::prelude::*;
use nadi_core::table::{ColumnAlign, Table};
use vte4::prelude::*;

// TODO make it better later

const NODE_COLOR: &str = "nodecolor";
const LINE_COLOR: &str = "linecolor";
const TEXT_COLOR: &str = "textcolor";
const LINE_WIDTH: &str = "linewidth";
const DEFAULT_LINE_WIDTH: f64 = 1.0;

pub fn calc_hw(net: &Network, ctx: &Context) -> (i32, i32) {
    match net.attr("drawtable") {
        Some(t) => match Table::from_attr(t) {
            Some(t) => return calc_table_hw(net, &t, ctx).unwrap_or((100, 100)),
            _ => (),
        },
        _ => (),
    }
    calc_net_hw(net, ctx)
}

pub fn calc_net_hw(net: &Network, ctx: &Context) -> (i32, i32) {
    ctx.set_font_size(14.0);
    let offx = 10.0;
    let offy = 10.0;
    let dely = 20.0;
    let delx = 40.0;
    let left = offx;
    let max_lev = net
        .nodes()
        .map(|n| n.lock().level())
        .max()
        .unwrap_or_default();

    let text_start = left + delx * max_lev as f64 + offx;
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

pub fn draw_network(
    net: &Network,
    ctx: &Context,
    w: i32,
    h: i32,
    darea: Option<&gtk::DrawingArea>,
) {
    if net.nodes_count() == 0 {
        return;
    }
    if let Some(da) = darea {
        let (h, w) = calc_hw(net, ctx);
        da.set_height_request(h);
        da.set_width_request(w);
    }
    match net.attr("drawtable") {
        Some(t) => match Table::try_from_attr(t) {
            Ok(t) => {
                let _ = draw_network_table(net, &t, ctx, w, h, darea);
                return;
            }
            Err(e) => {
                println!("{e:?}");
            }
        },
        _ => (),
    }
    draw_network_only(net, ctx, w, h, darea)
}
pub fn draw_network_only(
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
        let nx = left + delx * n.level() as f64;
        let ny = top - dely * n.index() as f64;
        ctx.move_to(nx, ny);
        _ = n.draw_color(ctx);
        if let RSome(o) = n.output() {
            let o = o.lock();
            ctx.move_to(nx, ny);
            set_node_color(&n, ctx, LINE_COLOR);
            set_line_width(&n, ctx, LINE_WIDTH);
            ctx.line_to(
                left + delx * o.level() as f64,
                top - dely * o.index() as f64,
            );
            _ = ctx.stroke();
        }
        ctx.move_to(text_start, ny);
        set_node_color(&n, ctx, TEXT_COLOR);
        let label = get_node_label(&n);
        _ = ctx.show_text(&label);
    }
}

pub fn calc_table_hw(net: &Network, table: &Table, ctx: &Context) -> anyhow::Result<(i32, i32)> {
    ctx.set_font_size(14.0);
    let headers: Vec<&str> = table.columns.iter().map(|c| c.header.as_str()).collect();
    let contents: Vec<Vec<String>> = table
        .render_contents(&net, false)?
        .into_iter()
        .rev()
        .collect();
    let header_widths: Vec<f64> = headers
        .iter()
        .map(|cell| {
            ctx.text_extents(cell)
                .map(|et| et.width())
                .unwrap_or_default()
        })
        .collect();
    let contents_widths: Vec<Vec<f64>> = contents
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    ctx.text_extents(cell)
                        .map(|et| et.width())
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect();
    let col_widths: Vec<f64> = header_widths
        .iter()
        .enumerate()
        .map(|(i, &h)| contents_widths.iter().map(|row| row[i]).fold(h, f64::max))
        .collect();
    let offx = 10.0;
    let dely = 20.0;
    let delx = 40.0;
    let twidth: f64 = col_widths.iter().sum::<f64>() + offx * (col_widths.len() + 1) as f64;
    let max_level = net.nodes().map(|n| n.lock().level()).max().unwrap_or(0);
    let width: f64 = delx * max_level as f64 + 2.0 * 5.0 + twidth + 2.0 * offx;
    let height: f64 = dely * (net.nodes_count() + 2) as f64 + 2.0 * 5.0;
    let w = width.ceil() as i32;
    let h = height.ceil() as i32;
    Ok((h, w))
}

pub fn draw_network_table(
    net: &Network,
    table: &Table,
    ctx: &Context,
    w: i32,
    h: i32,
    _darea: Option<&gtk::DrawingArea>,
) -> anyhow::Result<()> {
    // background
    if let Ok(c) = net
        .try_attr::<AttrColor>("bg_color")
        .and_then(|c| c.color())
    {
        ctx.save()?;
        c.set(ctx);
        ctx.paint()?;
        ctx.restore()?;
    }
    if let Ok(c) = net
        .try_attr::<AttrColor>("header_color")
        .and_then(|c| c.color())
    {
        c.set(ctx);
    } else {
        ctx.set_source_rgb(0.0, 0.0, 1.0);
    }
    ctx.set_font_size(14.0);
    let headers: Vec<&str> = table.columns.iter().map(|c| c.header.as_str()).collect();
    let contents: Vec<Vec<String>> = table
        .render_contents(&net, false)?
        .into_iter()
        .rev()
        .collect();
    let header_widths: Vec<f64> = headers
        .iter()
        .map(|cell| {
            ctx.text_extents(cell)
                .map(|et| et.width())
                .unwrap_or_default()
        })
        .collect();
    let contents_widths: Vec<Vec<f64>> = contents
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    ctx.text_extents(cell)
                        .map(|et| et.width())
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect();
    let alignments: Vec<&ColumnAlign> = table.columns.iter().map(|c| &c.align).collect();
    let max_level = net.nodes().map(|n| n.lock().level()).max().unwrap_or(0);

    let col_widths: Vec<f64> = header_widths
        .iter()
        .enumerate()
        .map(|(i, &h)| contents_widths.iter().map(|row| row[i]).fold(h, f64::max))
        .collect();
    let offx = 10.0;
    let dely = 20.0;
    let delx = 40.0;
    let mut height = h as f64;
    let width = w as f64;
    let twidth: f64 = col_widths.iter().sum::<f64>() + offx * (col_widths.len() + 1) as f64;
    let req_width = delx * max_level as f64 + 2.0 * 5.0 + twidth;
    let req_ht: f64 = dely * (net.nodes_count() + 2) as f64 + 2.0 * 5.0;
    let offset = (width - req_width) / 2.0;
    let txtstart = offset + delx * max_level as f64 + 2.0 * 5.0;
    let offset_y = (height - req_ht) / 2.0;
    height -= offset_y;
    let col_stops: Vec<f64> = (0..(col_widths.len()))
        .map(|i| col_widths[0..i].iter().sum::<f64>() + offx * (i + 1) as f64 + txtstart)
        .collect();
    for (i, (head, a)) in headers.iter().zip(&alignments).enumerate() {
        let stop = match a {
            ColumnAlign::Left => col_stops[i],
            ColumnAlign::Right => col_stops[i] + col_widths[i] - header_widths[i],
            ColumnAlign::Center => col_stops[i] + (col_widths[i] - header_widths[i]) / 2.0,
        };
        ctx.move_to(stop, offset_y + dely);
        ctx.show_text(head)?;
    }
    ctx.move_to(offset, offset_y + dely * 1.5);
    ctx.line_to(txtstart + twidth, offset_y + dely * 1.5);
    ctx.stroke()?;
    net.nodes_rev()
        .zip(contents)
        .zip(contents_widths)
        .try_for_each(|((n, row), row_widths)| -> cairo::Result<()> {
            let n = n.lock();
            let y = height - (n.index() + 1) as f64 * dely;
            let x = offset + n.level() as f64 * delx + offx / 2.0;

            if let RSome(o) = n.output() {
                set_node_color(&n, ctx, LINE_COLOR);
                let o = o.lock();
                let yo = height - (o.index() + 1) as f64 * dely;
                let xo = offset + o.level() as f64 * delx + offx / 2.0;
                let dx = xo - x;
                let dy = yo - y;
                let l = (dx.powi(2) + dy.powi(2)).sqrt();
                let (ux, uy) = (dx / l, dy / l);
                let (sx, sy) = (x + ux * 5.0 * 1.4, y + uy * 5.0 * 1.4);
                let (ex, ey) = (xo - ux * 5.0 * 1.4, yo - uy * 5.0 * 1.4);
                set_line_width(&n, ctx, LINE_WIDTH);
                ctx.move_to(sx, sy);
                ctx.line_to(ex, ey);
                ctx.stroke()?;
                let (asx, asy) = (ex - ux * 5.0, ey - uy * 5.0);
                let (aex, aey) = (xo - ux * 5.0, yo - uy * 5.0);
                ctx.set_line_width(DEFAULT_LINE_WIDTH);
                ctx.move_to(asx + uy * 5.0 * 0.5, asy - ux * 5.0 * 0.5);
                ctx.line_to(aex, aey);
                ctx.line_to(asx - uy * 5.0 * 0.5, asy + ux * 5.0 * 0.5);
                ctx.line_to(asx + ux, asy + uy);
                ctx.fill()?;
                ctx.stroke()?;
            }
            // if highlight.contains(&n.index()){
            // 	ctx.set_source_rgb(0.6, 0.35, 0.35);
            // } else {
            // 	ctx.set_source_rgb(0.35, 0.35, 0.6);
            // }
            ctx.move_to(x, y);
            n.draw_color(ctx)?;
            // set_node_color(&n, ctx, NODE_COLOR);
            // ctx.arc(x, y, 5.0, 0.0, 2.0 * 3.1416);
            // ctx.fill()?;
            // ctx.stroke()?;

            set_node_color(&n, ctx, TEXT_COLOR);
            for (i, (cell, a)) in row.iter().zip(&alignments).enumerate() {
                let stop = match a {
                    ColumnAlign::Left => col_stops[i],
                    ColumnAlign::Right => col_stops[i] + col_widths[i] - row_widths[i],
                    ColumnAlign::Center => col_stops[i] + (col_widths[i] - row_widths[i]) / 2.0,
                };
                ctx.move_to(stop, y);
                ctx.show_text(cell)?;
            }
            Ok(())
        })?;
    Ok(())
}

fn set_node_color(node: &NodeInner, ctx: &cairo::Context, attr: &str) {
    let c = node.try_attr::<AttrColor>(attr).unwrap_or_default();
    match c.color() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            crate::colors::Color::default()
        }
    }
    .set(ctx);
}

fn set_line_width(node: &NodeInner, ctx: &cairo::Context, attr: &str) {
    let w = node.try_attr::<f64>(attr).unwrap_or(DEFAULT_LINE_WIDTH);
    ctx.set_line_width(w)
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
