//! Shared egui shape painting for the Et Tu binaries.
//!
//! Included by both `src/main.rs` (`mod paint;`) and
//! `src/bin/generator.rs` (`#[path = "../paint.rs"] mod paint;`) so the
//! solver and generator render identical symbols.

use eframe::egui;
use et_tu::symbol_color;

pub fn color_word(v: u8) -> &'static str {
    match v {
        0 => "yellow",
        1 => "green",
        2 => "blue",
        3 => "red",
        4 => "brown",
        _ => "custom",
    }
}

pub fn egui_color(rgb: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub fn fill_for(sym: u8, show_colors: bool) -> egui::Color32 {
    if show_colors {
        egui_color(symbol_color(sym))
    } else {
        egui::Color32::from_rgb(45, 45, 48)
    }
}

/// Paint symbol `sym` centred in `rect` (canonical 5 shapes; otherwise number).
pub fn paint_symbol(
    painter: &egui::Painter,
    rect: egui::Rect,
    sym: u8,
    show_colors: bool,
    size_n: usize,
) {
    let fill = fill_for(sym, show_colors);
    let outline = egui::Stroke::new(1.5, egui::Color32::BLACK);
    if size_n != 5 || sym >= 5 {
        // generic fallback: coloured disc + number
        let c = rect.center();
        let r = rect.width().min(rect.height()) * 0.36;
        painter.circle_filled(c, r, fill);
        painter.circle_stroke(c, r, outline);
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            format!("{}", sym + 1),
            egui::FontId::proportional(r * 1.1),
            egui::Color32::BLACK,
        );
        return;
    }
    let c = rect.center();
    let w = rect.width();
    let h = rect.height();
    let r = w.min(h) * 0.36;
    match sym {
        2 => {
            // circle
            painter.circle_filled(c, r, fill);
            painter.circle_stroke(c, r, outline);
        }
        4 => {
            // square
            let sq = egui::Rect::from_center_size(c, egui::Vec2::splat(r * 1.55));
            painter.rect_filled(sq, 0.0, fill);
            painter.rect_stroke(sq, 0.0, outline, egui::StrokeKind::Inside);
        }
        1 => {
            // triangle (pointing up)
            let top = c + egui::Vec2::new(0.0, -r);
            let bl = c + egui::Vec2::new(-r * 0.95, r * 0.75);
            let br = c + egui::Vec2::new(r * 0.95, r * 0.75);
            painter.add(egui::Shape::convex_polygon(
                vec![top, br, bl],
                fill,
                outline,
            ));
        }
        3 => {
            // plus: two bars
            let t = r * 0.62;
            let vert = egui::Rect::from_center_size(c, egui::Vec2::new(t, r * 2.0));
            let horiz = egui::Rect::from_center_size(c, egui::Vec2::new(r * 2.0, t));
            painter.rect_filled(vert, 0.0, fill);
            painter.rect_filled(horiz, 0.0, fill);
            painter.rect_stroke(vert, 0.0, outline, egui::StrokeKind::Inside);
            painter.rect_stroke(horiz, 0.0, outline, egui::StrokeKind::Inside);
        }
        0 => {
            // 5-point star pointing up
            let mut pts = Vec::with_capacity(10);
            let inner = r * 0.42;
            for k in 0..10 {
                let rad = if k % 2 == 0 { r } else { inner };
                let ang = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
                pts.push(c + egui::Vec2::new(rad * ang.cos(), rad * ang.sin()));
            }
            painter.add(egui::Shape::convex_polygon(pts, fill, outline));
        }
        _ => {}
    }
}
