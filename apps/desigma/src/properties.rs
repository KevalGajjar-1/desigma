//! Right sidebar: Figma's "Design" tab.

use desigma_core::{Color, Editor, Node, NodeKind, Paint, Rect as DRect, TextAlign};
use egui::{Color32, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use crate::canvas::Canvas;
use crate::icons::{self, Icon};
use crate::theme;

pub fn ui(ui: &mut Ui, editor: &mut Editor, canvas: &mut Canvas) {
    header(ui, canvas);
    separator(ui);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            if editor.selection.is_empty() {
                page_section(ui, editor);
                return;
            }
            let primary = editor
                .selection
                .iter()
                .find_map(|id| editor.page().find(*id).cloned());
            let Some(primary) = primary else { return };
            if editor.selection.len() == 1 {
                type_section(ui, editor, &primary);
            } else {
                section_title(
                    ui,
                    &format!("{} layers selected", editor.selection.len()),
                    None,
                );
            }
            geometry_section(ui, editor, &primary);
            separator(ui);
            appearance_section(ui, editor, &primary);
            separator(ui);
            if primary.text().is_some() {
                text_section(ui, editor, &primary);
                separator(ui);
            }
            paints_section(ui, editor, &primary, PaintKind::Fill);
            separator(ui);
            paints_section(ui, editor, &primary, PaintKind::Stroke);
            separator(ui);
        });
}

fn header(ui: &mut Ui, canvas: &mut Canvas) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        tab(ui, "Design", true);
        tab(ui, "Prototype", false);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            let zoom = format!("{}%", (canvas.view.zoom * 100.0).round());
            ui.menu_button(RichText::new(zoom).font(theme::font(11.0)), |ui| {
                if ui.button("Zoom in              Ctrl +").clicked() {
                    canvas.zoom_by(2.0);
                }
                if ui.button("Zoom out             Ctrl -").clicked() {
                    canvas.zoom_by(0.5);
                }
                ui.separator();
                for (label, z) in [
                    ("Zoom to 50%", 0.5),
                    ("Zoom to 100%     Shift 0", 1.0),
                    ("Zoom to 200%", 2.0),
                ] {
                    if ui.button(label).clicked() {
                        canvas.set_zoom(z);
                    }
                }
            });
        });
    });
    ui.add_space(4.0);
}

fn tab(ui: &mut Ui, label: &str, active: bool) {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), theme::font_semibold(11.0), theme::TEXT);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(16.0, 16.0), Sense::hover());
    if active {
        ui.painter()
            .rect_filled(rect, theme::RADIUS, theme::HOVER_BG);
    }
    let color = if active {
        theme::TEXT
    } else {
        theme::TEXT_SECONDARY
    };
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
}

fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, theme::BORDER),
    );
}

/// Section title with an optional "+" button on the right. Returns true when "+" was clicked.
fn section_title(ui: &mut Ui, title: &str, plus: Option<&str>) -> bool {
    let mut clicked = false;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.set_min_height(theme::ROW_HEIGHT - 8.0);
        ui.add_space(16.0);
        ui.label(RichText::new(title).font(theme::font_semibold(11.0)));
        if let Some(tip) = plus {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                clicked = icons::icon_button(ui, Icon::Plus, false, 24.0)
                    .on_hover_text(tip)
                    .clicked();
            });
        }
    });
    clicked
}

fn row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(theme::INPUT_HEIGHT);
        ui.add_space(16.0);
        add(ui);
    });
}

/// A Figma-style numeric field: gray label followed by a draggable value.
fn num_field(
    ui: &mut Ui,
    label: &str,
    value: f64,
    width: f32,
    suffix: &str,
    range: std::ops::RangeInclusive<f64>,
) -> Option<f64> {
    let mut v = value;
    // A few labels are drawn as vector icons since Inter has no glyph for them.
    let icon = match label {
        "◐" => Some(Icon::Opacity),
        "◜" => Some(Icon::Radius),
        "≡" => Some(Icon::StrokeWeight),
        _ => None,
    };
    let prefix = if icon.is_some() {
        "     ".to_string()
    } else {
        format!("{label}  ")
    };
    let resp = ui.add_sized(
        [width, theme::INPUT_HEIGHT],
        egui::DragValue::new(&mut v)
            .prefix(prefix)
            .suffix(suffix)
            .speed(1.0)
            .range(range)
            .max_decimals(2),
    );
    if let Some(icon) = icon {
        let r = Rect::from_center_size(
            pos2(resp.rect.left() + 14.0, resp.rect.center().y),
            vec2(16.0, 16.0),
        );
        icons::paint(ui.painter(), r, icon, theme::TEXT_SECONDARY);
    }
    (resp.changed() && v != value).then_some(v)
}

fn page_section(ui: &mut Ui, editor: &mut Editor) {
    section_title(ui, "Page", None);
    let bg = editor.page().background;
    let mut new_bg = None;
    row(ui, |ui| {
        if let Some(c) = color_row(ui, "page-bg", bg) {
            new_bg = Some(c);
        }
    });
    if let Some(c) = new_bg {
        editor.checkpoint_coalesced("page-bg");
        editor.page_mut().background = c;
    }
    ui.add_space(8.0);
    separator(ui);
    section_title(ui, "Variables", None);
    row(ui, |ui| {
        ui.label(RichText::new("Local variables are coming soon").color(theme::TEXT_TERTIARY));
    });
}

fn type_section(ui: &mut Ui, editor: &mut Editor, n: &Node) {
    let title = n.kind.label();
    section_title(ui, title, None);
    if let NodeKind::Frame { clip_content } = n.kind {
        let mut clip = clip_content;
        row(ui, |ui| {
            if ui.checkbox(&mut clip, "Clip content").changed() {
                editor.edit_selected("clip", |n| {
                    if let NodeKind::Frame { clip_content } = &mut n.kind {
                        *clip_content = clip;
                    }
                });
                editor.end_coalescing();
            }
        });
    }
}

fn geometry_section(ui: &mut Ui, editor: &mut Editor, n: &Node) {
    let multi = editor.selection.len() > 1;
    let r = if multi {
        editor.selection_bounds().unwrap_or_default()
    } else {
        n.rect()
    };
    let w = (ui.available_width() - 16.0 - 16.0 - 8.0) / 2.0;
    section_title(ui, "Position", None);
    let mut new = r;
    row(ui, |ui| {
        if let Some(v) = num_field(ui, "X", r.x, w, "", f64::MIN..=f64::MAX) {
            new.x = v;
        }
        if let Some(v) = num_field(ui, "Y", r.y, w, "", f64::MIN..=f64::MAX) {
            new.y = v;
        }
    });
    section_title(ui, "Layout", None);
    row(ui, |ui| {
        if let Some(v) = num_field(ui, "W", r.w, w, "", 0.01..=1.0e6) {
            new.w = v;
        }
        if let Some(v) = num_field(ui, "H", r.h, w, "", 0.01..=1.0e6) {
            new.h = v;
        }
    });
    if new != r {
        editor.checkpoint_coalesced("geometry");
        if multi {
            let roots = editor.selection_roots();
            let originals: Vec<_> = roots
                .iter()
                .filter_map(|id| Some((*id, editor.page().world_rect(*id)?)))
                .collect();
            editor.resize_selection(&originals, r, new);
        } else {
            let id = n.id;
            let origin = editor.page().parent_origin(id);
            editor.set_world_rect(
                id,
                DRect::new(new.x + origin.x, new.y + origin.y, new.w, new.h),
            );
        }
    }
}

fn appearance_section(ui: &mut Ui, editor: &mut Editor, n: &Node) {
    section_title(ui, "Appearance", None);
    let w = (ui.available_width() - 16.0 - 16.0 - 8.0) / 2.0;
    let mut opacity = None;
    let mut radius = None;
    row(ui, |ui| {
        opacity = num_field(
            ui,
            "◐",
            (n.opacity * 100.0).round() as f64,
            w,
            "%",
            0.0..=100.0,
        );
        let has_radius = matches!(n.kind, NodeKind::Frame { .. } | NodeKind::Rectangle);
        if has_radius {
            radius = num_field(ui, "◜", n.corner_radius, w, "", 0.0..=1.0e5);
        }
    });
    if let Some(o) = opacity {
        editor.edit_selected("opacity", |n| n.opacity = (o / 100.0) as f32);
    }
    if let Some(r) = radius {
        editor.edit_selected("radius", |n| n.corner_radius = r);
    }
}

fn text_section(ui: &mut Ui, editor: &mut Editor, n: &Node) {
    let Some(t) = n.text().cloned() else { return };
    section_title(ui, "Typography", None);
    row(ui, |ui| {
        ui.add_sized(
            [ui.available_width() - 16.0, theme::INPUT_HEIGHT],
            egui::Label::new(RichText::new("Inter").color(theme::TEXT)).sense(Sense::hover()),
        );
    });
    let w = (ui.available_width() - 16.0 - 16.0 - 8.0) / 2.0;
    let mut weight = t.font_weight;
    let mut size = None;
    row(ui, |ui| {
        egui::ComboBox::from_id_salt("font-weight")
            .width(w - 8.0)
            .selected_text(weight_name(weight))
            .show_ui(ui, |ui| {
                for wv in [400u16, 500, 600] {
                    ui.selectable_value(&mut weight, wv, weight_name(wv));
                }
            });
        size = num_field(ui, "", t.font_size, w, "", 1.0..=1000.0);
    });
    if weight != t.font_weight {
        editor.edit_selected("font-weight", |n| {
            if let Some(t) = n.text_mut() {
                t.font_weight = weight;
            }
        });
        editor.end_coalescing();
    }
    if let Some(s) = size {
        editor.edit_selected("font-size", |n| {
            if let Some(t) = n.text_mut() {
                t.font_size = s;
            }
        });
    }
    let mut line_height = None;
    let mut align = t.align;
    row(ui, |ui| {
        let current = t.line_height.unwrap_or(t.font_size * 1.2);
        line_height = num_field(
            ui,
            "LH",
            (current * 100.0).round() / 100.0,
            w,
            "",
            1.0..=1000.0,
        );
        for (a, icon) in [
            (TextAlign::Left, Icon::AlignLeft),
            (TextAlign::Center, Icon::AlignCenter),
            (TextAlign::Right, Icon::AlignRight),
        ] {
            if icons::icon_button(ui, icon, align == a, 24.0).clicked() {
                align = a;
            }
        }
    });
    if let Some(lh) = line_height {
        editor.edit_selected("line-height", |n| {
            if let Some(t) = n.text_mut() {
                t.line_height = Some(lh);
            }
        });
    }
    if align != t.align {
        editor.edit_selected("align", |n| {
            if let Some(t) = n.text_mut() {
                t.align = align;
            }
        });
        editor.end_coalescing();
    }
    let mut auto = t.auto_width;
    row(ui, |ui| {
        if ui.checkbox(&mut auto, "Auto width").changed() {
            editor.edit_selected("auto-width", |n| {
                if let Some(t) = n.text_mut() {
                    t.auto_width = auto;
                }
            });
            editor.end_coalescing();
        }
    });
}

fn weight_name(w: u16) -> &'static str {
    match w {
        0..=449 => "Regular",
        450..=549 => "Medium",
        _ => "Semi Bold",
    }
}

#[derive(Clone, Copy, PartialEq)]
enum PaintKind {
    Fill,
    Stroke,
}

fn paints_section(ui: &mut Ui, editor: &mut Editor, n: &Node, kind: PaintKind) {
    let (title, list) = match kind {
        PaintKind::Fill => ("Fill", &n.fills),
        PaintKind::Stroke => ("Stroke", &n.strokes),
    };
    let key = title.to_lowercase();
    if section_title(
        ui,
        title,
        Some(if kind == PaintKind::Fill {
            "Add fill"
        } else {
            "Add stroke"
        }),
    ) {
        let default = match (kind, &n.kind) {
            (PaintKind::Stroke, _) => Color::BLACK,
            (_, NodeKind::Text(_)) => Color::BLACK,
            (_, NodeKind::Frame { .. }) => Color::WHITE,
            _ => Color::DEFAULT_SHAPE,
        };
        editor.edit_selected(&format!("{key}-add"), |n| {
            let list = if kind == PaintKind::Fill {
                &mut n.fills
            } else {
                &mut n.strokes
            };
            list.push(Paint::solid(default));
        });
        editor.end_coalescing();
    }
    let mut new_list = list.clone();
    let mut remove = None;
    let mut changed = false;
    // Figma lists the topmost paint first.
    for (i, paint) in list.iter().enumerate().rev() {
        let Paint::Solid { color, visible } = paint;
        row(ui, |ui| {
            let width = ui.available_width() - 16.0 - 56.0;
            ui.allocate_ui(vec2(width, theme::INPUT_HEIGHT), |ui| {
                if let Some(c) = color_row(ui, &format!("{key}-{i}"), *color) {
                    new_list[i] = Paint::Solid {
                        color: c,
                        visible: *visible,
                    };
                    changed = true;
                }
            });
            if icons::icon_button(
                ui,
                if *visible { Icon::Eye } else { Icon::EyeOff },
                false,
                24.0,
            )
            .clicked()
            {
                new_list[i] = Paint::Solid {
                    color: *color,
                    visible: !visible,
                };
                changed = true;
            }
            if icons::icon_button(ui, Icon::Minus, false, 24.0).clicked() {
                remove = Some(i);
            }
        });
    }
    if kind == PaintKind::Stroke && !list.is_empty() {
        let w = (ui.available_width() - 16.0 - 16.0 - 8.0) / 2.0;
        let mut weight = None;
        row(ui, |ui| {
            ui.add_sized(
                [w, theme::INPUT_HEIGHT],
                egui::Label::new(RichText::new("Inside").color(theme::TEXT_SECONDARY)),
            );
            weight = num_field(ui, "≡", n.stroke_weight, w, "", 0.0..=1000.0);
        });
        if let Some(wt) = weight {
            editor.edit_selected("stroke-weight", |n| n.stroke_weight = wt);
        }
    }
    if let Some(i) = remove {
        new_list.remove(i);
        changed = true;
    }
    if changed {
        editor.edit_selected(&key, |n| {
            if kind == PaintKind::Fill {
                n.fills = new_list.clone();
            } else {
                n.strokes = new_list.clone();
            }
        });
        if remove.is_some() {
            editor.end_coalescing();
        }
    }
}

/// Swatch + hex + opacity, like Figma's paint row. Returns the edited color.
fn color_row(ui: &mut Ui, id: &str, color: Color) -> Option<Color> {
    let mut out = None;
    let frame_w = ui.available_width().min(200.0);
    let (rect, _) = ui.allocate_exact_size(vec2(frame_w, theme::INPUT_HEIGHT), Sense::hover());
    ui.painter()
        .rect_filled(rect, theme::RADIUS, theme::INPUT_BG);

    // Swatch with popup color picker.
    let swatch =
        Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), vec2(14.0, 14.0));
    let sresp = ui.interact(swatch, ui.id().with((id, "swatch")), Sense::click());
    checker(ui, swatch);
    ui.painter()
        .rect_filled(swatch, 2.0, theme::to_color32(color));
    ui.painter().rect_stroke(
        swatch,
        2.0,
        Stroke::new(1.0, Color32::from_white_alpha(25)),
        egui::StrokeKind::Inside,
    );
    egui::Popup::from_toggle_button_response(&sresp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let mut c = theme::to_color32(color);
            if egui::color_picker::color_picker_color32(
                ui,
                &mut c,
                egui::color_picker::Alpha::OnlyBlend,
            ) {
                out = Some(theme::from_color32(c));
            }
        });

    // Hex.
    let hex_rect = Rect::from_min_max(
        pos2(swatch.right() + 8.0, rect.top()),
        pos2(rect.right() - 52.0, rect.bottom()),
    );
    let mem_id = ui.id().with((id, "hex"));
    let mut buf = ui
        .data(|d| d.get_temp::<String>(mem_id))
        .unwrap_or_else(|| color.to_hex());
    let te = ui.put(
        hex_rect,
        egui::TextEdit::singleline(&mut buf)
            .frame(egui::Frame::NONE)
            .font(theme::font(11.0))
            .vertical_align(egui::Align::Center),
    );
    if te.has_focus() {
        ui.data_mut(|d| d.insert_temp(mem_id, buf.clone()));
    } else {
        ui.data_mut(|d| d.remove::<String>(mem_id));
    }
    if te.lost_focus()
        && let Some(c) = Color::parse_hex(&buf, color.a)
    {
        out = Some(c);
    }

    // Opacity.
    let op_rect = Rect::from_min_max(pos2(rect.right() - 50.0, rect.top()), rect.right_bottom());
    ui.painter().line_segment(
        [
            pos2(op_rect.left(), rect.top()),
            pos2(op_rect.left(), rect.bottom()),
        ],
        Stroke::new(1.0, theme::PANEL_BG),
    );
    let mut pct = (color.a * 100.0).round() as f64;
    let resp = ui.put(
        op_rect.shrink2(vec2(4.0, 0.0)),
        egui::DragValue::new(&mut pct)
            .range(0.0..=100.0)
            .suffix("%")
            .speed(1.0),
    );
    if resp.changed() {
        out = Some(color.with_alpha((pct / 100.0) as f32));
    }
    out
}

fn checker(ui: &Ui, r: Rect) {
    let p = ui.painter();
    p.rect_filled(r, 2.0, Color32::WHITE);
    let s = r.width() / 2.0;
    p.rect_filled(
        Rect::from_min_size(r.min, vec2(s, s)),
        0.0,
        Color32::from_gray(204),
    );
    p.rect_filled(
        Rect::from_min_size(r.min + vec2(s, s), vec2(s, s)),
        0.0,
        Color32::from_gray(204),
    );
}
