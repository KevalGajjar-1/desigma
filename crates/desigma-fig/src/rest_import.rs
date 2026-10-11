//! Figma REST (`GET /v1/files/:key`) JSON -> [`Document`].
//!
//! The `.fig` importer converts its kiwi message into this same JSON shape and
//! reuses this mapper, so every field name below is the REST name.

use crate::svgpath::parse_path;
use crate::FigError;
use desigma_doc::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const MAX_DEPTH: usize = 300;

// ------------------------------------------------------------ JSON helpers

pub(crate) fn num(v: &Value) -> Option<f32> {
    v.as_f64().map(|x| x as f32)
}
pub(crate) fn fnum(v: &Value, k: &str) -> Option<f32> {
    v.get(k).and_then(num)
}
pub(crate) fn fstr<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}
pub(crate) fn fbool(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}
pub(crate) fn farr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

pub(crate) fn vec2(v: &Value) -> Option<Vec2> {
    Some(Vec2::new(fnum(v, "x")?, fnum(v, "y")?))
}

pub(crate) fn color(v: &Value) -> Option<Color> {
    Some(Color { r: fnum(v, "r")?, g: fnum(v, "g")?, b: fnum(v, "b")?, a: fnum(v, "a").unwrap_or(1.0) })
}

pub(crate) fn blend(s: &str) -> BlendMode {
    match s {
        "PASS_THROUGH" => BlendMode::PassThrough,
        "DARKEN" => BlendMode::Darken,
        "MULTIPLY" => BlendMode::Multiply,
        // No linear burn/dodge in the document model; closest relatives.
        "LINEAR_BURN" | "COLOR_BURN" => BlendMode::ColorBurn,
        "LIGHTEN" => BlendMode::Lighten,
        "SCREEN" => BlendMode::Screen,
        "LINEAR_DODGE" | "COLOR_DODGE" => BlendMode::ColorDodge,
        "OVERLAY" => BlendMode::Overlay,
        "SOFT_LIGHT" => BlendMode::SoftLight,
        "HARD_LIGHT" => BlendMode::HardLight,
        "DIFFERENCE" => BlendMode::Difference,
        "EXCLUSION" => BlendMode::Exclusion,
        "HUE" => BlendMode::Hue,
        "SATURATION" => BlendMode::Saturation,
        "COLOR" => BlendMode::Color,
        "LUMINOSITY" => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

// ------------------------------------------------------------ paints & effects

pub(crate) fn parse_paint(v: &Value) -> Option<Paint> {
    let t = fstr(v, "type")?;
    let handles = || -> [Vec2; 3] {
        let h = farr(v, "gradientHandlePositions");
        let g = |i: usize, d: Vec2| h.get(i).and_then(vec2).unwrap_or(d);
        [g(0, Vec2::new(0.5, 0.0)), g(1, Vec2::new(0.5, 1.0)), g(2, Vec2::new(1.0, 0.0))]
    };
    let stops = || -> Vec<GradientStop> {
        farr(v, "gradientStops")
            .iter()
            .filter_map(|s| Some(GradientStop { pos: fnum(s, "position")?, color: s.get("color").and_then(color)? }))
            .collect()
    };
    let kind = match t {
        "SOLID" => PaintKind::Solid(v.get("color").and_then(color).unwrap_or(Color::BLACK)),
        "GRADIENT_LINEAR" => PaintKind::LinearGradient { handles: handles(), stops: stops() },
        "GRADIENT_RADIAL" => PaintKind::RadialGradient { handles: handles(), stops: stops() },
        "GRADIENT_ANGULAR" => PaintKind::AngularGradient { handles: handles(), stops: stops() },
        "GRADIENT_DIAMOND" => PaintKind::DiamondGradient { handles: handles(), stops: stops() },
        "IMAGE" => PaintKind::Image {
            image_ref: fstr(v, "imageRef")?.to_string(),
            scale_mode: match fstr(v, "scaleMode") {
                Some("FIT") => ImageScaleMode::Fit,
                Some("TILE") => ImageScaleMode::Tile,
                Some("STRETCH") => ImageScaleMode::Crop,
                _ => ImageScaleMode::Fill,
            },
        },
        _ => return None, // EMOJI, VIDEO, ...
    };
    Some(Paint {
        kind,
        opacity: fnum(v, "opacity").unwrap_or(1.0),
        visible: fbool(v, "visible").unwrap_or(true),
        blend: fstr(v, "blendMode").map(blend).unwrap_or(BlendMode::Normal),
    })
}

pub(crate) fn parse_paints(v: Option<&Value>) -> Vec<Paint> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(parse_paint).collect()).unwrap_or_default()
}

fn parse_effect(v: &Value) -> Option<Effect> {
    let visible = fbool(v, "visible").unwrap_or(true);
    let radius = fnum(v, "radius").unwrap_or(0.0);
    let col = v.get("color").and_then(color).unwrap_or(Color::BLACK.with_alpha(0.25));
    let offset = v.get("offset").and_then(vec2).unwrap_or(Vec2::ZERO);
    let spread = fnum(v, "spread").unwrap_or(0.0);
    Some(match fstr(v, "type")? {
        "DROP_SHADOW" => Effect::DropShadow {
            color: col,
            offset,
            radius,
            spread,
            visible,
            show_behind: fbool(v, "showShadowBehindNode").unwrap_or(false),
        },
        "INNER_SHADOW" => Effect::InnerShadow { color: col, offset, radius, spread, visible },
        "LAYER_BLUR" => Effect::LayerBlur { radius, visible },
        "BACKGROUND_BLUR" => Effect::BackgroundBlur { radius, visible },
        _ => return None,
    })
}

fn parse_grid(v: &Value) -> Option<LayoutGrid> {
    let col = v.get("color").and_then(color).unwrap_or(Color::rgba(255, 0, 0, 25));
    let count = v.get("count").and_then(Value::as_u64).unwrap_or(5) as u32;
    let gutter = fnum(v, "gutterSize").unwrap_or(0.0);
    let offset = fnum(v, "offset").unwrap_or(0.0);
    match fstr(v, "pattern")? {
        "GRID" => Some(LayoutGrid::Grid { size: fnum(v, "sectionSize").unwrap_or(10.0), color: col }),
        "COLUMNS" => Some(LayoutGrid::Columns { count, gutter, offset, color: col }),
        "ROWS" => Some(LayoutGrid::Rows { count, gutter, offset, color: col }),
        _ => None,
    }
}

// ------------------------------------------------------------ text

/// Byte offset of each UTF-16 unit boundary: `out[u]` is the byte offset where
/// UTF-16 index `u` falls (low surrogates map to the end of their character).
pub(crate) fn utf16_to_byte_offsets(s: &str) -> Vec<usize> {
    let mut out = Vec::with_capacity(s.len() + 1);
    for (b, c) in s.char_indices() {
        out.push(b);
        if c.len_utf16() == 2 {
            out.push(b + c.len_utf8());
        }
    }
    out.push(s.len());
    out
}

/// Applies the (possibly partial) REST `TypeStyle` fields in `v` onto `ts`.
pub(crate) fn apply_type_style(ts: &mut TextStyle, v: &Value) {
    if let Some(f) = fstr(v, "fontFamily") {
        ts.font_family = f.to_string();
    }
    if let Some(w) = fnum(v, "fontWeight") {
        ts.font_weight = w.round().clamp(1.0, 1000.0) as u16;
    }
    if let Some(i) = fbool(v, "italic") {
        ts.italic = i;
    }
    if let Some(s) = fnum(v, "fontSize") {
        ts.font_size = s;
    }
    if let Some(a) = fstr(v, "textAlignHorizontal") {
        ts.align_h = match a {
            "CENTER" => TextAlignH::Center,
            "RIGHT" => TextAlignH::Right,
            "JUSTIFIED" => TextAlignH::Justified,
            _ => TextAlignH::Left,
        };
    }
    if let Some(a) = fstr(v, "textAlignVertical") {
        ts.align_v = match a {
            "CENTER" => TextAlignV::Center,
            "BOTTOM" => TextAlignV::Bottom,
            _ => TextAlignV::Top,
        };
    }
    if let Some(l) = fnum(v, "letterSpacing") {
        ts.letter_spacing = l;
    }
    if let Some(p) = fnum(v, "paragraphSpacing") {
        ts.paragraph_spacing = p;
    }
    match fstr(v, "lineHeightUnit") {
        Some("PIXELS") => {
            if let Some(px) = fnum(v, "lineHeightPx") {
                ts.line_height = LineHeight::Pixels(px);
            }
        }
        Some("FONT_SIZE_%") => {
            if let Some(p) = fnum(v, "lineHeightPercentFontSize").or_else(|| fnum(v, "lineHeightPercent")) {
                ts.line_height = LineHeight::Percent(p);
            }
        }
        Some(_) => ts.line_height = LineHeight::Auto,
        None => {
            if let Some(px) = fnum(v, "lineHeightPx") {
                ts.line_height = LineHeight::Pixels(px);
            }
        }
    }
    if let Some(c) = fstr(v, "textCase") {
        ts.case = match c {
            "UPPER" => TextCase::Upper,
            "LOWER" => TextCase::Lower,
            "TITLE" => TextCase::Title,
            "SMALL_CAPS" | "SMALL_CAPS_FORCED" => TextCase::SmallCaps,
            _ => TextCase::Original,
        };
    }
    if let Some(d) = fstr(v, "textDecoration") {
        ts.decoration = match d {
            "UNDERLINE" => TextDecoration::Underline,
            "STRIKETHROUGH" => TextDecoration::Strikethrough,
            _ => TextDecoration::None,
        };
    }
    if let Some(f) = v.get("fills") {
        ts.fills = parse_paints(Some(f));
    }
}

fn auto_resize(s: &str) -> TextAutoResize {
    match s {
        "WIDTH_AND_HEIGHT" => TextAutoResize::WidthAndHeight,
        "HEIGHT" => TextAutoResize::Height,
        "TRUNCATE" => TextAutoResize::Truncate,
        _ => TextAutoResize::None,
    }
}

// ------------------------------------------------------------ importer

struct Ctx {
    doc: Document,
    ids: BTreeMap<String, NodeId>,
    /// (instance, REST componentId)
    instances: Vec<(NodeId, String)>,
    component_keys: BTreeMap<String, String>,
}

#[derive(Clone, Copy)]
struct ParentInfo {
    /// Absolute bounding-box origin of the parent, for the no-transform fallback.
    origin: Vec2,
    layout: Option<LayoutDirection>,
}

/// Parses REST JSON text (rejecting pathologically deep nesting).
pub(crate) fn parse_json_text(json: &str) -> Result<Value, FigError> {
    let mut depth = 0usize;
    let mut in_str = false;
    let mut esc = false;
    for &b in json.as_bytes() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'[' | b'{' => {
                depth += 1;
                if depth > 600 {
                    return Err(FigError::Unsupported("JSON nesting too deep".into()));
                }
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let mut de = serde_json::Deserializer::from_str(json);
    de.disable_recursion_limit();
    let v = serde::Deserialize::deserialize(&mut de)?;
    de.end()?;
    Ok(v)
}

pub fn import_rest_json(json: &str) -> Result<Document, FigError> {
    let v = parse_json_text(json)?;
    import_rest_value(&v, false)
}

/// Maps an already-parsed REST-shaped value. `expand_instances` clones the
/// component subtree into childless instances (used by `.fig`, where instance
/// children are often not stored).
pub(crate) fn import_rest_value(v: &Value, expand_instances: bool) -> Result<Document, FigError> {
    let root = match v.get("document") {
        Some(d) => d,
        None if fstr(v, "type").is_some() => v,
        None => return Err(FigError::Format("REST JSON has no `document`".into())),
    };
    let mut cx = Ctx {
        doc: Document::new(fstr(v, "name").unwrap_or("Untitled")),
        ids: BTreeMap::new(),
        instances: Vec::new(),
        component_keys: BTreeMap::new(),
    };
    if let Some(comps) = v.get("components").and_then(Value::as_object) {
        for (id, c) in comps {
            if let Some(k) = fstr(c, "key") {
                cx.component_keys.insert(id.clone(), k.to_string());
            }
        }
    }
    let pages: Vec<&Value> = match fstr(root, "type") {
        Some("CANVAS") => vec![root],
        _ => farr(root, "children").iter().filter(|c| fstr(c, "type") == Some("CANVAS")).collect(),
    };
    for p in pages {
        cx.import_page(p);
    }
    cx.finish(expand_instances);
    Ok(cx.doc)
}

impl Ctx {
    fn import_page(&mut self, v: &Value) {
        let pid = self.doc.add_page(fstr(v, "name").unwrap_or("Page"));
        if let Some(src) = fstr(v, "id") {
            self.ids.insert(src.to_string(), pid);
        }
        let bg = v.get("backgroundColor").and_then(color).unwrap_or(Color::rgb(0xE5, 0xE5, 0xE5));
        if let Some(n) = self.doc.get_mut(pid) {
            n.kind = NodeKind::Page { background: bg.with_alpha(1.0) };
            n.source_id = fstr(v, "id").map(String::from);
        }
        let info = ParentInfo { origin: Vec2::ZERO, layout: None };
        for c in farr(v, "children") {
            self.import_node(c, pid, info, 0);
        }
    }

    fn import_node(&mut self, v: &Value, parent: NodeId, pinfo: ParentInfo, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let Some(ty) = fstr(v, "type") else { return };
        if ty == "CANVAS" || ty == "DOCUMENT" {
            return;
        }
        let id = self.doc.alloc_id();
        let (mut node, my_layout) = self.build_node(id, ty, v, pinfo);
        node.source_id = fstr(v, "id").map(String::from);
        if let Some(src) = &node.source_id {
            self.ids.insert(src.clone(), id);
        }
        let is_instance = matches!(node.kind, NodeKind::Instance { .. });
        let origin = abs_origin(v).unwrap_or(pinfo.origin);
        let container = node.kind.is_container();
        if self.doc.insert(parent, node).is_err() {
            return;
        }
        if is_instance {
            if let Some(cid) = fstr(v, "componentId") {
                self.instances.push((id, cid.to_string()));
            }
        }
        if container {
            let info = ParentInfo { origin, layout: my_layout };
            for c in farr(v, "children") {
                self.import_node(c, id, info, depth + 1);
            }
        }
    }

    /// Returns the node and, for auto-layout frames, its direction.
    fn build_node(&mut self, id: NodeId, ty: &str, v: &Value, pinfo: ParentInfo) -> (Node, Option<LayoutDirection>) {
        let frame = || frame_props(v);
        let kind = match ty {
            "FRAME" => NodeKind::Frame(frame()),
            "GROUP" => NodeKind::Group,
            "SECTION" => NodeKind::Section,
            "COMPONENT" => NodeKind::Component(frame()),
            "COMPONENT_SET" => NodeKind::ComponentSet(frame()),
            "INSTANCE" => NodeKind::Instance {
                frame: frame(),
                component: None,
                component_key: fstr(v, "componentId")
                    .and_then(|c| self.component_keys.get(c).cloned())
                    .or_else(|| fstr(v, "componentKey").map(String::from)),
            },
            "RECTANGLE" => NodeKind::Rectangle,
            "ELLIPSE" => {
                let a = v.get("arcData");
                let d = ArcData::default();
                NodeKind::Ellipse(ArcData {
                    start: a.and_then(|a| fnum(a, "startingAngle")).unwrap_or(d.start),
                    end: a.and_then(|a| fnum(a, "endingAngle")).unwrap_or(d.end),
                    inner_radius: a.and_then(|a| fnum(a, "innerRadius")).unwrap_or(0.0),
                })
            }
            "LINE" => NodeKind::Line,
            "REGULAR_POLYGON" => NodeKind::Polygon { count: v.get("pointCount").and_then(Value::as_u64).unwrap_or(3).clamp(3, 1000) as u32 },
            "STAR" => NodeKind::Star {
                count: v.get("pointCount").and_then(Value::as_u64).unwrap_or(5).clamp(3, 1000) as u32,
                inner_ratio: fnum(v, "starInnerRadius").unwrap_or(0.382),
            },
            "VECTOR" => NodeKind::Vector { paths: parse_geometry(farr(v, "fillGeometry")) },
            "TEXT" => NodeKind::Text(text_props(v)),
            "BOOLEAN_OPERATION" => NodeKind::BooleanOp(match fstr(v, "booleanOperation") {
                Some("SUBTRACT") => BooleanOp::Subtract,
                Some("INTERSECT") => BooleanOp::Intersect,
                Some("EXCLUDE") => BooleanOp::Exclude,
                _ => BooleanOp::Union,
            }),
            other => NodeKind::Unknown { type_name: other.to_string() },
        };
        let is_text = matches!(kind, NodeKind::Text(_));
        let my_layout = kind.frame_props().and_then(|f| f.auto_layout).map(|a| a.direction);
        let default_blend = if matches!(kind, NodeKind::Rectangle | NodeKind::Ellipse(_) | NodeKind::Line | NodeKind::Vector { .. } | NodeKind::Text(_)) {
            BlendMode::Normal
        } else {
            BlendMode::PassThrough
        };
        let mut n = Node::new(id, kind);
        if let Some(s) = fstr(v, "name") {
            n.name = s.to_string();
        }
        n.visible = fbool(v, "visible").unwrap_or(true);
        n.locked = fbool(v, "locked").unwrap_or(false);
        n.opacity = fnum(v, "opacity").unwrap_or(1.0);
        n.blend = fstr(v, "blendMode").map(blend).unwrap_or(default_blend);
        n.is_mask = fbool(v, "isMask").unwrap_or(false);

        // Placement.
        let bbox = v.get("absoluteBoundingBox");
        n.transform = match v.get("relativeTransform").and_then(parse_transform) {
            Some(t) => t,
            None => match bbox {
                Some(b) => Affine::translate(fnum(b, "x").unwrap_or(0.0) - pinfo.origin.x, fnum(b, "y").unwrap_or(0.0) - pinfo.origin.y),
                None => Affine::IDENTITY,
            },
        };
        n.size = v
            .get("size")
            .and_then(vec2)
            .or_else(|| bbox.and_then(|b| Some(Vec2::new(fnum(b, "width")?, fnum(b, "height")?))))
            .unwrap_or(Vec2::ZERO);

        // Paint & stroke.
        let fills = parse_paints(v.get("fills"));
        // Text keeps its color in `TextStyle::fills` (set by `text_props`).
        if !is_text {
            n.fills = fills;
        }
        n.stroke.paints = parse_paints(v.get("strokes"));
        if let Some(w) = fnum(v, "strokeWeight") {
            n.stroke.weight = w;
        }
        if let Some(i) = v.get("individualStrokeWeights") {
            let g = |k: &str| fnum(i, k).unwrap_or(0.0);
            n.stroke.side_weights = Some([g("top"), g("right"), g("bottom"), g("left")]);
        }
        n.stroke.align = match fstr(v, "strokeAlign") {
            Some("CENTER") => StrokeAlign::Center,
            Some("OUTSIDE") => StrokeAlign::Outside,
            _ => StrokeAlign::Inside,
        };
        n.stroke.cap = match fstr(v, "strokeCap") {
            Some("ROUND") => StrokeCap::Round,
            Some("SQUARE") => StrokeCap::Square,
            _ => StrokeCap::None,
        };
        n.stroke.join = match fstr(v, "strokeJoin") {
            Some("BEVEL") => StrokeJoin::Bevel,
            Some("ROUND") => StrokeJoin::Round,
            _ => StrokeJoin::Miter,
        };
        if let Some(l) = fnum(v, "strokeMiterLimit") {
            n.stroke.miter_limit = l;
        } else if let Some(deg) = fnum(v, "strokeMiterAngle") {
            let s = (deg.to_radians() / 2.0).sin();
            if s > 1e-4 {
                n.stroke.miter_limit = 1.0 / s;
            }
        }
        n.stroke.dashes = farr(v, "strokeDashes").iter().filter_map(num).collect();
        n.effects = farr(v, "effects").iter().filter_map(parse_effect).collect();

        // Corners.
        if let Some(r) = v.get("rectangleCornerRadii").and_then(Value::as_array).filter(|a| a.len() == 4) {
            for (i, x) in r.iter().enumerate() {
                n.corner_radii[i] = num(x).unwrap_or(0.0);
            }
        } else if let Some(r) = fnum(v, "cornerRadius") {
            n.corner_radii = [r; 4];
        }
        n.corner_smoothing = fnum(v, "cornerSmoothing").unwrap_or(0.0);

        // Constraints.
        if let Some(c) = v.get("constraints") {
            n.constraints = Constraints {
                horizontal: match fstr(c, "horizontal") {
                    Some("RIGHT") => Constraint::Max,
                    Some("CENTER") => Constraint::Center,
                    Some("LEFT_RIGHT") => Constraint::Stretch,
                    Some("SCALE") => Constraint::Scale,
                    _ => Constraint::Min,
                },
                vertical: match fstr(c, "vertical") {
                    Some("BOTTOM") => Constraint::Max,
                    Some("CENTER") => Constraint::Center,
                    Some("TOP_BOTTOM") => Constraint::Stretch,
                    Some("SCALE") => Constraint::Scale,
                    _ => Constraint::Min,
                },
            };
        }

        n.layout_child = layout_child(v, pinfo.layout, &n.kind);

        // Style references and library style definitions.
        if let Some(styles) = v.get("styles").and_then(Value::as_object) {
            for (k, sid) in styles {
                let Some(sid) = sid.as_str() else { continue };
                let key = match k.as_str() {
                    "fills" | "fill" => "fill",
                    "strokes" | "stroke" => "stroke",
                    "text" => "text",
                    "effect" | "effects" => "effect",
                    "grid" | "grids" => "grid",
                    other => other,
                };
                n.style_refs.insert(key.to_string(), sid.to_string());
                if self.doc.styles.contains_key(sid) {
                    continue;
                }
                let def = match key {
                    "fill" => Some(Style::Fill(match &n.kind {
                        NodeKind::Text(t) => t.style.fills.clone(),
                        _ => n.fills.clone(),
                    })),
                    "stroke" => Some(Style::Fill(n.stroke.paints.clone())),
                    "text" => match &n.kind {
                        NodeKind::Text(t) => Some(Style::Text(t.style.clone())),
                        _ => None,
                    },
                    "effect" => Some(Style::Effect(n.effects.clone())),
                    "grid" => n.kind.frame_props().map(|f| Style::Grid(f.grids.clone())),
                    _ => None,
                };
                if let Some(d) = def {
                    self.doc.styles.insert(sid.to_string(), d);
                }
            }
        }
        (n, my_layout)
    }

    fn finish(&mut self, expand_instances: bool) {
        let links = std::mem::take(&mut self.instances);
        for (inst, cid) in links {
            let Some(&comp) = self.ids.get(&cid) else { continue };
            if let Some(NodeKind::Instance { component, .. }) = self.doc.get_mut(inst).map(|n| &mut n.kind) {
                *component = Some(comp);
            }
            if expand_instances {
                self.expand_instance(inst, comp);
            }
        }
    }

    /// Deep-copies the component's children under an empty instance.
    fn expand_instance(&mut self, inst: NodeId, comp: NodeId) {
        if !self.doc.get(inst).map(|n| n.children.is_empty()).unwrap_or(false) || self.doc.is_ancestor(comp, inst) || comp == inst {
            return;
        }
        let prefix = self.doc.get(inst).and_then(|n| n.source_id.clone()).unwrap_or_default();
        let kids = self.doc.get(comp).map(|n| n.children.clone()).unwrap_or_default();
        let mut budget = 50_000usize;
        for k in kids {
            self.clone_subtree(k, inst, &prefix, &mut budget, 0);
        }
    }

    fn clone_subtree(&mut self, src: NodeId, parent: NodeId, prefix: &str, budget: &mut usize, depth: usize) {
        if *budget == 0 || depth > MAX_DEPTH {
            return;
        }
        *budget -= 1;
        let Some(orig) = self.doc.get(src).cloned() else { return };
        let id = self.doc.alloc_id();
        let mut n = orig.clone();
        n.id = id;
        n.children.clear();
        n.parent = None;
        n.source_id = orig.source_id.as_ref().map(|s| format!("I{prefix};{s}"));
        let container = n.kind.is_container();
        if self.doc.insert(parent, n).is_err() {
            return;
        }
        if container {
            for c in orig.children {
                self.clone_subtree(c, id, prefix, budget, depth + 1);
            }
        }
    }
}

fn abs_origin(v: &Value) -> Option<Vec2> {
    let b = v.get("absoluteBoundingBox")?;
    Some(Vec2::new(fnum(b, "x")?, fnum(b, "y")?))
}

fn parse_transform(v: &Value) -> Option<Affine> {
    let r = v.as_array()?;
    let row = |i: usize| -> Option<[f32; 3]> {
        let a = r.get(i)?.as_array()?;
        Some([num(a.first()?)?, num(a.get(1)?)?, num(a.get(2)?)?])
    };
    let (r0, r1) = (row(0)?, row(1)?);
    Some(Affine { a: r0[0], c: r0[1], tx: r0[2], b: r1[0], d: r1[1], ty: r1[2] })
}

fn parse_geometry(items: &[Value]) -> Vec<Path> {
    items
        .iter()
        .filter_map(|g| {
            let d = fstr(g, "path")?;
            let rule = match fstr(g, "windingRule") {
                Some("EVENODD") => FillRule::EvenOdd,
                _ => FillRule::NonZero,
            };
            Some(parse_path(d, rule))
        })
        .collect()
}

fn frame_props(v: &Value) -> FrameProps {
    let dir = match fstr(v, "layoutMode") {
        Some("HORIZONTAL") => Some(LayoutDirection::Horizontal),
        Some("VERTICAL") => Some(LayoutDirection::Vertical),
        _ => None,
    };
    let auto_layout = dir.map(|direction| {
        let align = |s: Option<&str>| match s {
            Some("CENTER") => LayoutAlign::Center,
            Some("MAX") => LayoutAlign::End,
            Some("SPACE_BETWEEN") => LayoutAlign::SpaceBetween,
            Some("BASELINE") => LayoutAlign::Baseline,
            _ => LayoutAlign::Start,
        };
        let sizing = |s: Option<&str>| if s == Some("AUTO") { Sizing::Hug } else { Sizing::Fixed };
        let p = |k: &str| fnum(v, k).unwrap_or(0.0);
        AutoLayout {
            direction,
            wrap: fstr(v, "layoutWrap") == Some("WRAP"),
            item_spacing: p("itemSpacing"),
            counter_spacing: p("counterAxisSpacing"),
            padding: [p("paddingTop"), p("paddingRight"), p("paddingBottom"), p("paddingLeft")],
            primary_align: align(fstr(v, "primaryAxisAlignItems")),
            counter_align: align(fstr(v, "counterAxisAlignItems")),
            primary_sizing: sizing(fstr(v, "primaryAxisSizingMode")),
            counter_sizing: sizing(fstr(v, "counterAxisSizingMode")),
            stroke_included: fbool(v, "strokesIncludedInLayout").unwrap_or(false),
            reverse_z: fbool(v, "itemReverseZIndex").unwrap_or(false),
        }
    });
    FrameProps { clips_content: fbool(v, "clipsContent").unwrap_or(false), auto_layout, grids: farr(v, "layoutGrids").iter().filter_map(parse_grid).collect() }
}

fn text_props(v: &Value) -> TextProps {
    let content = fstr(v, "characters").unwrap_or("").to_string();
    let mut style = TextStyle::default();
    let st = v.get("style");
    if let Some(st) = st {
        apply_type_style(&mut style, st);
    }
    if v.get("fills").is_some() {
        style.fills = parse_paints(v.get("fills"));
    }
    let auto = fstr(v, "textAutoResize").or_else(|| st.and_then(|s| fstr(s, "textAutoResize"))).map(auto_resize).unwrap_or(TextAutoResize::None);

    let mut runs = Vec::new();
    let overrides: Vec<u64> = farr(v, "characterStyleOverrides").iter().map(|x| x.as_u64().unwrap_or(0)).collect();
    if overrides.iter().any(|&x| x != 0) {
        let table = v.get("styleOverrideTable");
        let offs = utf16_to_byte_offsets(&content);
        let units = offs.len() - 1;
        let mut i = 0;
        while i < overrides.len().min(units) {
            let sid = overrides[i];
            let mut j = i + 1;
            while j < overrides.len().min(units) && overrides[j] == sid {
                j += 1;
            }
            if sid != 0 {
                if let Some(o) = table.and_then(|t| t.get(sid.to_string())) {
                    let mut rs = style.clone();
                    apply_type_style(&mut rs, o);
                    let (s, e) = (offs[i], offs[j]);
                    if e > s {
                        runs.push(TextRun { start: s, end: e, style: rs });
                    }
                }
            }
            i = j;
        }
    }
    TextProps { content, style, runs, auto_resize: auto }
}

fn layout_child(v: &Value, parent: Option<LayoutDirection>, kind: &NodeKind) -> LayoutChild {
    let sizing = |s: Option<&str>| match s {
        Some("HUG") => Some(Sizing::Hug),
        Some("FILL") => Some(Sizing::Fill),
        Some("FIXED") => Some(Sizing::Fixed),
        _ => None,
    };
    let grow = fnum(v, "layoutGrow").unwrap_or(0.0);
    let mut lc = LayoutChild {
        absolute: fstr(v, "layoutPositioning") == Some("ABSOLUTE"),
        grow,
        min_w: fnum(v, "minWidth"),
        max_w: fnum(v, "maxWidth"),
        min_h: fnum(v, "minHeight"),
        max_h: fnum(v, "maxHeight"),
        ..Default::default()
    };
    let (sh, sv) = (sizing(fstr(v, "layoutSizingHorizontal")), sizing(fstr(v, "layoutSizingVertical")));
    // Derive from the legacy fields when the explicit ones are absent.
    let mut h = Sizing::Fixed;
    let mut vt = Sizing::Fixed;
    let own_hug = |dir_h: bool| -> bool {
        match kind {
            NodeKind::Text(t) => match t.auto_resize {
                TextAutoResize::WidthAndHeight => true,
                TextAutoResize::Height => !dir_h,
                _ => false,
            },
            k => match k.frame_props().and_then(|f| f.auto_layout) {
                Some(al) => {
                    let along = (al.direction == LayoutDirection::Horizontal) == dir_h;
                    (if along { al.primary_sizing } else { al.counter_sizing }) == Sizing::Hug
                }
                None => false,
            },
        }
    };
    if own_hug(true) {
        h = Sizing::Hug;
    }
    if own_hug(false) {
        vt = Sizing::Hug;
    }
    if let Some(dir) = parent {
        let stretch = fstr(v, "layoutAlign") == Some("STRETCH");
        let (prim_fill, counter_fill) = (grow > 0.0, stretch);
        match dir {
            LayoutDirection::Horizontal => {
                if prim_fill {
                    h = Sizing::Fill;
                }
                if counter_fill {
                    vt = Sizing::Fill;
                }
            }
            LayoutDirection::Vertical => {
                if prim_fill {
                    vt = Sizing::Fill;
                }
                if counter_fill {
                    h = Sizing::Fill;
                }
            }
        }
    }
    lc.horizontal = sh.unwrap_or(h);
    lc.vertical = sv.unwrap_or(vt);
    lc
}

/// Every `imageRef` used by fills, strokes and text styles, sorted and deduplicated.
pub fn image_refs(doc: &Document) -> Vec<String> {
    let mut set = BTreeSet::new();
    let mut add = |ps: &[Paint]| {
        for p in ps {
            if let PaintKind::Image { image_ref, .. } = &p.kind {
                set.insert(image_ref.clone());
            }
        }
    };
    for n in doc.nodes.values() {
        add(&n.fills);
        add(&n.stroke.paints);
        if let NodeKind::Text(t) = &n.kind {
            add(&t.style.fills);
            for r in &t.runs {
                add(&r.style.fills);
            }
        }
    }
    for s in doc.styles.values() {
        match s {
            Style::Fill(p) => add(p),
            Style::Text(t) => add(&t.fills),
            _ => {}
        }
    }
    set.into_iter().collect()
}
