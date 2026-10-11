//! `.fig` (fig-kiwi) import.
//!
//! Pipeline: container -> kiwi schema + message -> REST-shaped JSON tree ->
//! the same mapper as the REST importer. Field names follow community
//! reverse engineering of Figma's schema; everything is optional and unknown
//! fields are ignored.

use crate::container::{self, Archive};
use crate::kiwi::{self, KiwiValue, Schema};
use crate::rest_import::{self, fnum, fstr};
use crate::svgpath::write_path;
use crate::FigError;
use desigma_doc::{Affine, Document, FillRule, Path, PathCmd, Vec2};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

struct Decoded {
    archive: Archive,
    message: KiwiValue,
}

fn decode(bytes: &[u8]) -> Result<Decoded, FigError> {
    let archive = container::open(bytes)?;
    let (schema_bin, msg_bin) = container::read_chunks(&archive.canvas)?;
    let schema = Schema::decode(&schema_bin).map_err(|e| FigError::Format(format!("kiwi schema: {e}")))?;
    let root = schema.root_message().ok_or_else(|| FigError::Format("schema has no message definition".into()))?;
    let message = schema.decode_message(root, &msg_bin).map_err(|e| FigError::Format(format!("kiwi message: {e}")))?;
    Ok(Decoded { archive, message })
}

/// Raw kiwi message as JSON (enums by name, byte arrays as hex) for debugging.
pub fn decode_fig_to_json(bytes: &[u8]) -> Result<Value, FigError> {
    let d = decode(bytes)?;
    let mut out = Map::new();
    out.insert("message".into(), kiwi::to_json(&d.message));
    if let Some(m) = d.archive.meta {
        out.insert("meta".into(), m);
    }
    out.insert("images".into(), Value::Array(d.archive.images.keys().map(|k| Value::String(k.clone())).collect()));
    Ok(Value::Object(out))
}

/// Imports a `.fig` file (raw fig-kiwi or zip-wrapped) from bytes.
pub fn import_fig(bytes: &[u8]) -> Result<Document, FigError> {
    let d = decode(bytes)?;
    let name = d
        .archive
        .meta
        .as_ref()
        .and_then(|m| fstr(m, "file_name").or_else(|| fstr(m, "name")))
        .unwrap_or("Untitled")
        .to_string();
    let rest = message_to_rest(&d.message, &name);
    let mut doc = rest_import::import_rest_value(&rest, true)?;
    doc.images = d.archive.images;
    Ok(doc)
}

// ------------------------------------------------------------------ tree

struct Entry {
    id: String,
    parent: Option<String>,
    pos: String,
    json: Value,
}

fn guid_str(g: &Value) -> Option<String> {
    Some(format!("{}:{}", g.get("sessionID")?.as_u64()?, g.get("localID")?.as_u64()?))
}

/// Builds the REST-shaped `{name, document, ...}` value from a decoded message.
pub(crate) fn message_to_rest(msg: &KiwiValue, name: &str) -> Value {
    let blobs: Vec<&[u8]> = msg
        .get("blobs")
        .and_then(KiwiValue::as_array)
        .map(|a| a.iter().map(|b| b.get("bytes").and_then(KiwiValue::as_bytes).unwrap_or(&[])).collect())
        .unwrap_or_default();

    let mut entries: Vec<Entry> = Vec::new();
    for nc in msg.get("nodeChanges").and_then(KiwiValue::as_array).unwrap_or(&[]) {
        let j = kiwi::to_json(nc);
        if fstr(&j, "phase") == Some("REMOVED") {
            continue;
        }
        let Some(id) = j.get("guid").and_then(guid_str) else { continue };
        let parent = j.get("parentIndex").and_then(|p| p.get("guid")).and_then(guid_str);
        let pos = j.get("parentIndex").and_then(|p| fstr(p, "position")).unwrap_or("").to_string();
        entries.push(Entry { id, parent, pos, json: j });
    }

    let index: HashMap<&str, usize> = entries.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        match e.parent.as_deref().and_then(|p| index.get(p)) {
            Some(&p) if p != i => children.entry(p).or_default().push(i),
            _ => roots.push(i),
        }
    }
    for list in children.values_mut() {
        list.sort_by(|a, b| entries[*a].pos.cmp(&entries[*b].pos));
    }

    let doc_idx = roots.iter().copied().find(|&i| fstr(&entries[i].json, "type") == Some("DOCUMENT"));
    let canvases: Vec<usize> = match doc_idx {
        Some(d) => children.get(&d).cloned().unwrap_or_default(),
        None => roots.iter().copied().filter(|&i| fstr(&entries[i].json, "type") == Some("CANVAS")).collect(),
    };

    let mut visited: HashSet<usize> = HashSet::new();
    let pages: Vec<Value> = canvases
        .into_iter()
        .filter(|&i| entries[i].json.get("internalOnly").and_then(Value::as_bool) != Some(true))
        .filter_map(|i| build(&entries, &children, &blobs, i, &mut visited, 0))
        .collect();

    json!({
        "name": name,
        "document": {"id": "0:0", "name": "Document", "type": "DOCUMENT", "children": pages},
    })
}

fn build(
    entries: &[Entry],
    children: &HashMap<usize, Vec<usize>>,
    blobs: &[&[u8]],
    i: usize,
    visited: &mut HashSet<usize>,
    depth: usize,
) -> Option<Value> {
    if depth > 250 || !visited.insert(i) {
        return None;
    }
    let e = &entries[i];
    let mut m = convert_node(&e.json, &e.id, blobs)?;
    let kids: Vec<Value> = children
        .get(&i)
        .map(|c| c.iter().filter_map(|&k| build(entries, children, blobs, k, visited, depth + 1)).collect())
        .unwrap_or_default();
    if !kids.is_empty() || matches!(fstr(&e.json, "type"), Some("CANVAS" | "FRAME" | "GROUP" | "SYMBOL" | "INSTANCE" | "SECTION" | "BOOLEAN_OPERATION")) {
        m.insert("children".into(), Value::Array(kids));
    }
    Some(Value::Object(m))
}

// ------------------------------------------------------------------ node

fn copy(m: &mut Map<String, Value>, from: &Value, src: &str, dst: &str) {
    if let Some(v) = from.get(src) {
        m.insert(dst.into(), v.clone());
    }
}

fn enum_map(m: &mut Map<String, Value>, from: &Value, src: &str, dst: &str, table: &[(&str, &str)]) {
    if let Some(s) = fstr(from, src) {
        let out = table.iter().find(|(k, _)| *k == s).map(|(_, v)| *v).unwrap_or(s);
        m.insert(dst.into(), out.into());
    }
}

fn convert_node(nc: &Value, id: &str, blobs: &[&[u8]]) -> Option<Map<String, Value>> {
    let ty = match fstr(nc, "type")? {
        "DOCUMENT" | "NONE" => return None,
        "SYMBOL" => "COMPONENT",
        "ROUNDED_RECTANGLE" => "RECTANGLE",
        "FRAME" if nc.get("isStateGroup").and_then(Value::as_bool) == Some(true) => "COMPONENT_SET",
        other => other,
    };
    let mut m = Map::new();
    m.insert("id".into(), id.into());
    m.insert("type".into(), ty.into());
    copy(&mut m, nc, "name", "name");
    copy(&mut m, nc, "visible", "visible");
    copy(&mut m, nc, "locked", "locked");
    copy(&mut m, nc, "opacity", "opacity");
    copy(&mut m, nc, "blendMode", "blendMode");
    copy(&mut m, nc, "mask", "isMask");
    copy(&mut m, nc, "size", "size");
    if let Some(t) = nc.get("transform") {
        let g = |k: &str, d: f64| t.get(k).and_then(Value::as_f64).unwrap_or(d);
        m.insert("relativeTransform".into(), json!([[g("m00", 1.0), g("m01", 0.0), g("m02", 0.0)], [g("m10", 0.0), g("m11", 1.0), g("m12", 0.0)]]));
    }
    if ty == "CANVAS" {
        copy(&mut m, nc, "backgroundColor", "backgroundColor");
        return Some(m);
    }

    // Paint.
    let paints = |k: &str| -> Value { Value::Array(rest_import::farr(nc, k).iter().filter_map(convert_paint).collect()) };
    m.insert("fills".into(), paints("fillPaints"));
    m.insert("strokes".into(), paints("strokePaints"));
    copy(&mut m, nc, "strokeWeight", "strokeWeight");
    copy(&mut m, nc, "strokeAlign", "strokeAlign");
    enum_map(&mut m, nc, "strokeCap", "strokeCap", &[("ARROW_LINES", "NONE"), ("ARROW_EQUILATERAL", "NONE")]);
    copy(&mut m, nc, "strokeJoin", "strokeJoin");
    copy(&mut m, nc, "dashPattern", "strokeDashes");
    copy(&mut m, nc, "strokeMiterAngle", "strokeMiterAngle");
    copy(&mut m, nc, "miterLimit", "strokeMiterLimit");
    if nc.get("borderStrokeWeightsIndependent").and_then(Value::as_bool) == Some(true) {
        let w = fnum(nc, "strokeWeight").unwrap_or(1.0);
        let g = |k: &str| fnum(nc, k).unwrap_or(w);
        m.insert(
            "individualStrokeWeights".into(),
            json!({"top": g("borderTopWeight"), "right": g("borderRightWeight"), "bottom": g("borderBottomWeight"), "left": g("borderLeftWeight")}),
        );
    }
    let effects: Vec<Value> = rest_import::farr(nc, "effects").iter().filter_map(convert_effect).collect();
    m.insert("effects".into(), Value::Array(effects));

    // Corners.
    copy(&mut m, nc, "cornerRadius", "cornerRadius");
    copy(&mut m, nc, "cornerSmoothing", "cornerSmoothing");
    if nc.get("rectangleCornerRadiiIndependent").and_then(Value::as_bool) == Some(true) {
        let base = fnum(nc, "cornerRadius").unwrap_or(0.0);
        let g = |k: &str| fnum(nc, k).unwrap_or(base);
        m.insert(
            "rectangleCornerRadii".into(),
            json!([g("rectangleTopLeftCornerRadius"), g("rectangleTopRightCornerRadius"), g("rectangleBottomRightCornerRadius"), g("rectangleBottomLeftCornerRadius")]),
        );
    }

    // Constraints.
    let h = match fstr(nc, "horizontalConstraint") {
        Some("CENTER") => "CENTER",
        Some("MAX") => "RIGHT",
        Some("STRETCH") => "LEFT_RIGHT",
        Some("SCALE") => "SCALE",
        _ => "LEFT",
    };
    let v = match fstr(nc, "verticalConstraint") {
        Some("CENTER") => "CENTER",
        Some("MAX") => "BOTTOM",
        Some("STRETCH") => "TOP_BOTTOM",
        Some("SCALE") => "SCALE",
        _ => "TOP",
    };
    m.insert("constraints".into(), json!({"horizontal": h, "vertical": v}));

    // Frames / auto layout.
    if matches!(ty, "FRAME" | "COMPONENT" | "COMPONENT_SET" | "INSTANCE") {
        let masked = nc.get("frameMaskDisabled").and_then(Value::as_bool) != Some(true);
        m.insert("clipsContent".into(), masked.into());
        convert_stack(&mut m, nc);
    }
    // Child layout.
    if let Some(g) = fnum(nc, "stackChildPrimaryGrow") {
        m.insert("layoutGrow".into(), g.into());
    }
    if fstr(nc, "stackChildAlignSelf") == Some("STRETCH") {
        m.insert("layoutAlign".into(), "STRETCH".into());
    }
    if fstr(nc, "stackPositioning") == Some("ABSOLUTE") {
        m.insert("layoutPositioning".into(), "ABSOLUTE".into());
    }
    for (src, dst_x, dst_y) in [("minSize", "minWidth", "minHeight"), ("maxSize", "maxWidth", "maxHeight")] {
        if let Some(s) = nc.get(src) {
            let s = s.get("value").unwrap_or(s);
            if let Some(x) = fnum(s, "x") {
                m.insert(dst_x.into(), x.into());
            }
            if let Some(y) = fnum(s, "y") {
                m.insert(dst_y.into(), y.into());
            }
        }
    }

    // Style references.
    let mut styles = Map::new();
    for (src, dst) in
        [("styleIdForFill", "fill"), ("styleIdForStrokeFill", "stroke"), ("styleIdForText", "text"), ("styleIdForEffect", "effect"), ("styleIdForGrid", "grid")]
    {
        if let Some(g) = nc.get(src).and_then(|s| s.get("guid")).and_then(guid_str) {
            styles.insert(dst.into(), g.into());
        }
    }
    if !styles.is_empty() {
        m.insert("styles".into(), Value::Object(styles));
    }

    // Type-specific.
    match ty {
        "ELLIPSE" => copy(&mut m, nc, "arcData", "arcData"),
        "STAR" => {
            if let Some(c) = nc.get("count").or_else(|| nc.get("pointCount")) {
                m.insert("pointCount".into(), c.clone());
            }
            copy(&mut m, nc, "starInnerScale", "starInnerRadius");
        }
        "REGULAR_POLYGON" => {
            if let Some(c) = nc.get("count").or_else(|| nc.get("pointCount")) {
                m.insert("pointCount".into(), c.clone());
            }
        }
        "BOOLEAN_OPERATION" => enum_map(&mut m, nc, "booleanOperation", "booleanOperation", &[("XOR", "EXCLUDE")]),
        "INSTANCE" => {
            if let Some(g) = nc.get("symbolData").and_then(|s| s.get("symbolID")).and_then(guid_str) {
                m.insert("componentId".into(), g.into());
            }
        }
        "VECTOR" => {
            let geo: Vec<Value> = rest_import::farr(nc, "fillGeometry")
                .iter()
                .filter_map(|g| {
                    let idx = g.get("commandsBlob")?.as_u64()? as usize;
                    let path = commands_blob_to_path(blobs.get(idx)?, fstr(g, "windingRule") == Some("EVENODD"));
                    Some(json!({"path": write_path(&path), "windingRule": fstr(g, "windingRule").unwrap_or("NONZERO")}))
                })
                .collect();
            m.insert("fillGeometry".into(), Value::Array(geo));
        }
        "TEXT" => convert_text(&mut m, nc),
        _ => {}
    }
    Some(m)
}

fn convert_stack(m: &mut Map<String, Value>, nc: &Value) {
    let mode = match fstr(nc, "stackMode") {
        Some("HORIZONTAL") => "HORIZONTAL",
        Some("VERTICAL") => "VERTICAL",
        _ => return,
    };
    m.insert("layoutMode".into(), mode.into());
    copy(m, nc, "stackSpacing", "itemSpacing");
    copy(m, nc, "stackCounterSpacing", "counterAxisSpacing");
    let h = fnum(nc, "stackHorizontalPadding");
    let v = fnum(nc, "stackVerticalPadding");
    let ins = |m: &mut Map<String, Value>, k: &str, x: Option<f32>| {
        if let Some(x) = x {
            m.insert(k.into(), x.into());
        }
    };
    ins(m, "paddingLeft", h);
    ins(m, "paddingRight", fnum(nc, "stackPaddingRight").or(h));
    ins(m, "paddingTop", v);
    ins(m, "paddingBottom", fnum(nc, "stackPaddingBottom").or(v));
    let align = |s: Option<&str>| -> &'static str {
        match s {
            Some("CENTER") => "CENTER",
            Some("MAX") => "MAX",
            Some("SPACE_BETWEEN") | Some("SPACE_EVENLY") => "SPACE_BETWEEN",
            Some("BASELINE") => "BASELINE",
            _ => "MIN",
        }
    };
    m.insert("primaryAxisAlignItems".into(), align(fstr(nc, "stackPrimaryAlignItems")).into());
    m.insert("counterAxisAlignItems".into(), align(fstr(nc, "stackCounterAlignItems")).into());
    let sizing = |s: Option<&str>| if s.map(|s| s.starts_with("RESIZE_TO_FIT")).unwrap_or(false) { "AUTO" } else { "FIXED" };
    m.insert("primaryAxisSizingMode".into(), sizing(fstr(nc, "stackPrimarySizing")).into());
    m.insert("counterAxisSizingMode".into(), sizing(fstr(nc, "stackCounterSizing")).into());
    if nc.get("stackWrap").and_then(Value::as_bool) == Some(true) {
        m.insert("layoutWrap".into(), "WRAP".into());
    }
    copy(m, nc, "stackReverseZIndex", "itemReverseZIndex");
    copy(m, nc, "bordersTakeSpace", "strokesIncludedInLayout");
}

// ------------------------------------------------------------------ paint / effect

fn convert_paint(p: &Value) -> Option<Value> {
    let ty = fstr(p, "type")?;
    let mut m = Map::new();
    m.insert("type".into(), ty.into());
    copy(&mut m, p, "opacity", "opacity");
    copy(&mut m, p, "visible", "visible");
    copy(&mut m, p, "blendMode", "blendMode");
    match ty {
        "SOLID" => copy(&mut m, p, "color", "color"),
        t if t.starts_with("GRADIENT") => {
            let stops: Vec<Value> = rest_import::farr(p, "stops").iter().map(|s| json!({"position": s.get("position"), "color": s.get("color")})).collect();
            m.insert("gradientStops".into(), Value::Array(stops));
            let t = p.get("transform");
            let g = |k: &str, d: f32| t.and_then(|t| fnum(t, k)).unwrap_or(d);
            let xf = Affine { a: g("m00", 1.0), c: g("m01", 0.0), tx: g("m02", 0.0), b: g("m10", 0.0), d: g("m11", 1.0), ty: g("m12", 0.0) };
            let inv = xf.invert().unwrap_or(Affine::IDENTITY);
            let h = [Vec2::new(0.0, 0.5), Vec2::new(1.0, 0.5), Vec2::new(0.0, 1.0)].map(|q| {
                let r = inv.apply(q);
                json!({"x": kiwi::clean_f32(r.x), "y": kiwi::clean_f32(r.y)})
            });
            m.insert("gradientHandlePositions".into(), Value::Array(h.to_vec()));
        }
        "IMAGE" => {
            let hash = p.get("image").and_then(|i| fstr(i, "hash")).or_else(|| fstr(p, "imageHash"))?;
            m.insert("imageRef".into(), hash.into());
            enum_map(&mut m, p, "imageScaleMode", "scaleMode", &[]);
        }
        _ => return None,
    }
    Some(Value::Object(m))
}

fn convert_effect(e: &Value) -> Option<Value> {
    let ty = match fstr(e, "type")? {
        "FOREGROUND_BLUR" => "LAYER_BLUR",
        other => other,
    };
    let mut m = Map::new();
    m.insert("type".into(), ty.into());
    for k in ["color", "offset", "radius", "spread", "visible", "showShadowBehindNode"] {
        copy(&mut m, e, k, k);
    }
    Some(Value::Object(m))
}

// ------------------------------------------------------------------ text

/// "SemiBold Italic" -> (600, true)
pub(crate) fn weight_from_style_name(style: &str) -> (u16, bool) {
    let norm: String = style.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    let italic = norm.contains("italic") || norm.contains("oblique");
    const TABLE: &[(&str, u16)] = &[
        ("extralight", 200),
        ("ultralight", 200),
        ("extrabold", 800),
        ("ultrabold", 800),
        ("semibold", 600),
        ("demibold", 600),
        ("thin", 100),
        ("hairline", 100),
        ("light", 300),
        ("medium", 500),
        ("bold", 700),
        ("black", 900),
        ("heavy", 900),
        ("regular", 400),
        ("normal", 400),
        ("book", 400),
    ];
    let w = TABLE.iter().find(|(k, _)| norm.contains(k)).map(|(_, w)| *w).unwrap_or(400);
    (w, italic)
}

/// Partial REST `TypeStyle` from kiwi text properties (`base_size` resolves
/// percentage letter spacing for override entries without their own size).
fn convert_type_style(o: &Value, base_size: Option<f32>) -> Map<String, Value> {
    let mut m = Map::new();
    if let Some(f) = o.get("fontName") {
        if let Some(fam) = fstr(f, "family") {
            m.insert("fontFamily".into(), fam.into());
        }
        if let Some(st) = fstr(f, "style") {
            let (w, i) = weight_from_style_name(st);
            m.insert("fontWeight".into(), w.into());
            m.insert("italic".into(), i.into());
        }
    }
    let size = fnum(o, "fontSize").or(base_size);
    if let Some(s) = fnum(o, "fontSize") {
        m.insert("fontSize".into(), s.into());
    }
    copy(&mut m, o, "textAlignHorizontal", "textAlignHorizontal");
    copy(&mut m, o, "textAlignVertical", "textAlignVertical");
    copy(&mut m, o, "textCase", "textCase");
    copy(&mut m, o, "textDecoration", "textDecoration");
    copy(&mut m, o, "paragraphSpacing", "paragraphSpacing");
    if let Some(ls) = o.get("letterSpacing") {
        let v = fnum(ls, "value").unwrap_or(0.0);
        let px = if fstr(ls, "units") == Some("PERCENT") { v / 100.0 * size.unwrap_or(0.0) } else { v };
        m.insert("letterSpacing".into(), kiwi::clean_f32(px));
    }
    if let Some(lh) = o.get("lineHeight") {
        let v = fnum(lh, "value").unwrap_or(100.0);
        match fstr(lh, "units") {
            Some("PIXELS") => {
                m.insert("lineHeightUnit".into(), "PIXELS".into());
                m.insert("lineHeightPx".into(), kiwi::clean_f32(v));
            }
            Some("RAW") => {
                m.insert("lineHeightUnit".into(), "FONT_SIZE_%".into());
                m.insert("lineHeightPercentFontSize".into(), kiwi::clean_f32(v * 100.0));
            }
            _ => {
                if (v - 100.0).abs() < 1e-3 {
                    m.insert("lineHeightUnit".into(), "INTRINSIC_%".into());
                } else {
                    m.insert("lineHeightUnit".into(), "FONT_SIZE_%".into());
                    m.insert("lineHeightPercentFontSize".into(), kiwi::clean_f32(v));
                }
            }
        }
    }
    if o.get("fillPaints").is_some() {
        let fills: Vec<Value> = rest_import::farr(o, "fillPaints").iter().filter_map(convert_paint).collect();
        m.insert("fills".into(), Value::Array(fills));
    }
    m
}

fn convert_text(m: &mut Map<String, Value>, nc: &Value) {
    let td = nc.get("textData");
    let chars = td.and_then(|t| fstr(t, "characters")).unwrap_or("");
    m.insert("characters".into(), chars.into());
    let mut style = convert_type_style(nc, None);
    style.remove("fills"); // node-level fills are the text color
    copy(&mut style, nc, "textAutoResize", "textAutoResize");
    if let Some(a) = fstr(nc, "textAutoResize") {
        m.insert("textAutoResize".into(), a.into());
    }
    m.insert("style".into(), Value::Object(style));
    let base_size = fnum(nc, "fontSize");
    if let Some(td) = td {
        if let Some(ids) = td.get("characterStyleIDs").and_then(Value::as_array) {
            m.insert("characterStyleOverrides".into(), Value::Array(ids.clone()));
            let mut table = Map::new();
            for o in rest_import::farr(td, "styleOverrideTable") {
                if let Some(sid) = o.get("styleID").and_then(Value::as_u64) {
                    table.insert(sid.to_string(), Value::Object(convert_type_style(o, base_size)));
                }
            }
            m.insert("styleOverrideTable".into(), Value::Object(table));
        }
    }
}

// ------------------------------------------------------------------ blobs

/// Decodes a Figma path commands blob (0=close, 1=move, 2=line, 3=quad, 4=cubic; f32 LE coords).
pub fn commands_blob_to_path(b: &[u8], even_odd: bool) -> Path {
    let mut cmds = Vec::new();
    let mut i = 0usize;
    let f = |i: usize| -> Option<f32> { b.get(i..i + 4).map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]])) };
    let pt = |i: usize| -> Option<Vec2> { Some(Vec2::new(f(i)?, f(i + 4)?)) };
    while i < b.len() {
        let op = b[i];
        i += 1;
        let ok = match op {
            0 => {
                cmds.push(PathCmd::Close);
                Some(())
            }
            1 => pt(i).map(|p| {
                cmds.push(PathCmd::MoveTo(p));
                i += 8;
            }),
            2 => pt(i).map(|p| {
                cmds.push(PathCmd::LineTo(p));
                i += 8;
            }),
            3 => pt(i).zip(pt(i + 8)).map(|(c, p)| {
                cmds.push(PathCmd::QuadTo(c, p));
                i += 16;
            }),
            4 => pt(i).zip(pt(i + 8)).zip(pt(i + 16)).map(|((c1, c2), p)| {
                cmds.push(PathCmd::CubicTo(c1, c2, p));
                i += 24;
            }),
            _ => None,
        };
        if ok.is_none() {
            break;
        }
    }
    Path { cmds, fill_rule: if even_odd { FillRule::EvenOdd } else { FillRule::NonZero } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names() {
        assert_eq!(weight_from_style_name("Regular"), (400, false));
        assert_eq!(weight_from_style_name("Semi Bold Italic"), (600, true));
        assert_eq!(weight_from_style_name("ExtraBold"), (800, false));
        assert_eq!(weight_from_style_name("Bold"), (700, false));
        assert_eq!(weight_from_style_name("Extra-Light"), (200, false));
    }

    #[test]
    fn blob_decoding_is_tolerant() {
        let mut b = vec![1u8];
        for v in [1.0f32, 2.0] {
            b.extend(v.to_le_bytes());
        }
        b.push(2);
        b.extend(3.0f32.to_le_bytes()); // truncated lineTo
        let p = commands_blob_to_path(&b, false);
        assert_eq!(p.cmds, vec![PathCmd::MoveTo(Vec2::new(1.0, 2.0))]);
        assert!(commands_blob_to_path(&[9, 9, 9], false).cmds.is_empty());
    }
}
