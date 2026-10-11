//! Test support: a Figma-like kiwi schema and a synthetic message, so `.fig`
//! files can be synthesized without any real Figma file. Not part of the
//! stable API.

use crate::container::{self, Compression};
use crate::kiwi::{DefKind, FieldType, KiwiValue as V, Native, Schema, SchemaBuilder};
use std::collections::BTreeMap;

fn enum_def(b: &mut SchemaBuilder, name: &str, variants: &[&str]) {
    let d = b.define(name, DefKind::Enum);
    for (i, v) in variants.iter().enumerate() {
        b.variant(d, v, i as u32);
    }
}

/// A reduced version of Figma's schema (same names for the fields we read).
pub fn figma_like_schema() -> Schema {
    use Native::*;
    let mut b = SchemaBuilder::new();
    // Enum index 0 must exist: enum fields reference definition 0 in the binary form.
    enum_def(&mut b, "Phase", &["CREATED", "REMOVED"]);
    enum_def(
        &mut b,
        "NodeType",
        &[
            "NONE", "DOCUMENT", "CANVAS", "GROUP", "FRAME", "BOOLEAN_OPERATION", "VECTOR", "STAR", "LINE", "ELLIPSE", "RECTANGLE",
            "REGULAR_POLYGON", "ROUNDED_RECTANGLE", "TEXT", "SLICE", "SYMBOL", "INSTANCE", "SECTION", "STICKY",
        ],
    );
    enum_def(&mut b, "PaintType", &["SOLID", "GRADIENT_LINEAR", "GRADIENT_RADIAL", "GRADIENT_ANGULAR", "GRADIENT_DIAMOND", "IMAGE"]);
    enum_def(&mut b, "StrokeAlign", &["CENTER", "INSIDE", "OUTSIDE"]);
    enum_def(&mut b, "StrokeJoin", &["MITER", "BEVEL", "ROUND"]);
    enum_def(&mut b, "StrokeCap", &["NONE", "ROUND", "SQUARE", "ARROW_LINES", "ARROW_EQUILATERAL"]);
    enum_def(&mut b, "EffectType", &["INNER_SHADOW", "DROP_SHADOW", "FOREGROUND_BLUR", "BACKGROUND_BLUR"]);
    enum_def(&mut b, "StackMode", &["NONE", "HORIZONTAL", "VERTICAL"]);
    enum_def(&mut b, "StackAlign", &["MIN", "CENTER", "MAX", "SPACE_BETWEEN", "BASELINE"]);
    enum_def(&mut b, "StackSize", &["FIXED", "RESIZE_TO_FIT", "RESIZE_TO_FIT_WITH_IMPLICIT_SIZE"]);
    enum_def(&mut b, "StackCounterAlign", &["STRETCH", "AUTO"]);
    enum_def(&mut b, "StackPositioning", &["AUTO", "ABSOLUTE"]);
    enum_def(&mut b, "TextAlignHorizontal", &["LEFT", "CENTER", "RIGHT", "JUSTIFIED"]);
    enum_def(&mut b, "TextAlignVertical", &["TOP", "CENTER", "BOTTOM"]);
    enum_def(&mut b, "TextAutoResize", &["NONE", "WIDTH_AND_HEIGHT", "HEIGHT"]);
    enum_def(&mut b, "TextCase", &["ORIGINAL", "UPPER", "LOWER", "TITLE"]);
    enum_def(&mut b, "TextDecoration", &["NONE", "UNDERLINE", "STRIKETHROUGH"]);
    enum_def(&mut b, "NumberUnits", &["RAW", "PIXELS", "PERCENT"]);
    enum_def(&mut b, "BlendMode", &["PASS_THROUGH", "NORMAL", "DARKEN", "MULTIPLY"]);
    enum_def(&mut b, "ImageScaleMode", &["STRETCH", "FIT", "FILL", "TILE"]);
    enum_def(&mut b, "WindingRule", &["NONZERO", "ODD"]);
    enum_def(&mut b, "ConstraintType", &["MIN", "CENTER", "MAX", "STRETCH", "SCALE"]);
    enum_def(&mut b, "BooleanOperation", &["UNION", "INTERSECT", "SUBTRACT", "XOR"]);

    let nat = FieldType::Native;
    let def = |b: &SchemaBuilder, n: &str| FieldType::Def(b.index(n));

    let g = b.define("GUID", DefKind::Struct);
    b.field(g, "sessionID", nat(Uint), false, 1);
    b.field(g, "localID", nat(Uint), false, 2);
    let vec = b.define("Vector", DefKind::Struct);
    b.field(vec, "x", nat(Float), false, 1);
    b.field(vec, "y", nat(Float), false, 2);
    let mx = b.define("Matrix", DefKind::Struct);
    for (i, n) in ["m00", "m01", "m02", "m10", "m11", "m12"].iter().enumerate() {
        b.field(mx, n, nat(Float), false, i as u32 + 1);
    }
    let c = b.define("Color", DefKind::Struct);
    for (i, n) in ["r", "g", "b", "a"].iter().enumerate() {
        b.field(c, n, nat(Float), false, i as u32 + 1);
    }
    let pi = b.define("ParentIndex", DefKind::Struct);
    let gt = def(&b, "GUID");
    b.field(pi, "guid", gt, false, 1);
    b.field(pi, "position", nat(String), false, 2);
    let cs = b.define("ColorStop", DefKind::Struct);
    let ct = def(&b, "Color");
    b.field(cs, "color", ct, false, 1);
    b.field(cs, "position", nat(Float), false, 2);
    let img = b.define("Image", DefKind::Message);
    b.field(img, "hash", nat(Byte), true, 1);
    b.field(img, "name", nat(String), false, 2);
    let num = b.define("Number", DefKind::Struct);
    b.field(num, "value", nat(Float), false, 1);
    let nu = def(&b, "NumberUnits");
    b.field(num, "units", nu, false, 2);
    let fnm = b.define("FontName", DefKind::Struct);
    b.field(fnm, "family", nat(String), false, 1);
    b.field(fnm, "style", nat(String), false, 2);
    b.field(fnm, "postscript", nat(String), false, 3);
    let arc = b.define("ArcData", DefKind::Struct);
    b.field(arc, "startingAngle", nat(Float), false, 1);
    b.field(arc, "endingAngle", nat(Float), false, 2);
    b.field(arc, "innerRadius", nat(Float), false, 3);
    let sid = b.define("StyleId", DefKind::Message);
    let gt = def(&b, "GUID");
    b.field(sid, "guid", gt, false, 1);

    let paint = b.define("Paint", DefKind::Message);
    let t = def(&b, "PaintType");
    b.field(paint, "type", t, false, 1);
    let ct = def(&b, "Color");
    b.field(paint, "color", ct, false, 2);
    b.field(paint, "opacity", nat(Float), false, 3);
    b.field(paint, "visible", nat(Bool), false, 4);
    let t = def(&b, "BlendMode");
    b.field(paint, "blendMode", t, false, 5);
    let cst = def(&b, "ColorStop");
    b.field(paint, "stops", cst, true, 6);
    let t = def(&b, "Matrix");
    b.field(paint, "transform", t, false, 7);
    let t = def(&b, "Image");
    b.field(paint, "image", t, false, 8);
    let t = def(&b, "ImageScaleMode");
    b.field(paint, "imageScaleMode", t, false, 9);

    let eff = b.define("Effect", DefKind::Message);
    let t = def(&b, "EffectType");
    b.field(eff, "type", t, false, 1);
    let ct = def(&b, "Color");
    b.field(eff, "color", ct, false, 2);
    let t = def(&b, "Vector");
    b.field(eff, "offset", t, false, 3);
    b.field(eff, "radius", nat(Float), false, 4);
    b.field(eff, "spread", nat(Float), false, 5);
    b.field(eff, "visible", nat(Bool), false, 6);
    b.field(eff, "showShadowBehindNode", nat(Bool), false, 7);

    let fg = b.define("FillGeometry", DefKind::Message);
    let t = def(&b, "WindingRule");
    b.field(fg, "windingRule", t, false, 1);
    b.field(fg, "commandsBlob", nat(Uint), false, 2);

    let so = b.define("StyleOverride", DefKind::Message);
    b.field(so, "styleID", nat(Uint), false, 1);
    b.field(so, "fontSize", nat(Float), false, 2);
    let t = def(&b, "FontName");
    b.field(so, "fontName", t, false, 3);
    let t = def(&b, "Paint");
    b.field(so, "fillPaints", t, true, 4);
    let t = def(&b, "TextDecoration");
    b.field(so, "textDecoration", t, false, 5);

    let td = b.define("TextData", DefKind::Message);
    b.field(td, "characters", nat(String), false, 1);
    b.field(td, "characterStyleIDs", nat(Uint), true, 2);
    let t = def(&b, "StyleOverride");
    b.field(td, "styleOverrideTable", t, true, 3);

    let nc = b.define("NodeChange", DefKind::Message);
    let mut id = 0;
    let mut f = |b: &mut SchemaBuilder, name: &str, ty: FieldType, arr: bool| {
        id += 1;
        b.field(nc, name, ty, arr, id);
    };
    let (gt, pit, nt, ph) = (def(&b, "GUID"), def(&b, "ParentIndex"), def(&b, "NodeType"), def(&b, "Phase"));
    f(&mut b, "guid", gt, false);
    f(&mut b, "phase", ph, false);
    f(&mut b, "parentIndex", pit, false);
    f(&mut b, "type", nt, false);
    f(&mut b, "name", nat(String), false);
    f(&mut b, "visible", nat(Bool), false);
    f(&mut b, "locked", nat(Bool), false);
    f(&mut b, "opacity", nat(Float), false);
    let (v, m, bm) = (def(&b, "Vector"), def(&b, "Matrix"), def(&b, "BlendMode"));
    f(&mut b, "size", v, false);
    f(&mut b, "transform", m, false);
    f(&mut b, "blendMode", bm, false);
    let (p, e) = (def(&b, "Paint"), def(&b, "Effect"));
    f(&mut b, "fillPaints", p, true);
    f(&mut b, "strokePaints", p, true);
    f(&mut b, "effects", e, true);
    f(&mut b, "strokeWeight", nat(Float), false);
    let (sa, sj, sc) = (def(&b, "StrokeAlign"), def(&b, "StrokeJoin"), def(&b, "StrokeCap"));
    f(&mut b, "strokeAlign", sa, false);
    f(&mut b, "strokeJoin", sj, false);
    f(&mut b, "strokeCap", sc, false);
    f(&mut b, "dashPattern", nat(Float), true);
    f(&mut b, "cornerRadius", nat(Float), false);
    f(&mut b, "cornerSmoothing", nat(Float), false);
    f(&mut b, "rectangleCornerRadiiIndependent", nat(Bool), false);
    for n in ["rectangleTopLeftCornerRadius", "rectangleTopRightCornerRadius", "rectangleBottomLeftCornerRadius", "rectangleBottomRightCornerRadius"] {
        f(&mut b, n, nat(Float), false);
    }
    f(&mut b, "mask", nat(Bool), false);
    f(&mut b, "frameMaskDisabled", nat(Bool), false);
    f(&mut b, "internalOnly", nat(Bool), false);
    f(&mut b, "isStateGroup", nat(Bool), false);
    let c = def(&b, "Color");
    f(&mut b, "backgroundColor", c, false);
    let (sm, al, sz) = (def(&b, "StackMode"), def(&b, "StackAlign"), def(&b, "StackSize"));
    f(&mut b, "stackMode", sm, false);
    f(&mut b, "stackSpacing", nat(Float), false);
    f(&mut b, "stackCounterSpacing", nat(Float), false);
    f(&mut b, "stackHorizontalPadding", nat(Float), false);
    f(&mut b, "stackVerticalPadding", nat(Float), false);
    f(&mut b, "stackPaddingRight", nat(Float), false);
    f(&mut b, "stackPaddingBottom", nat(Float), false);
    f(&mut b, "stackPrimaryAlignItems", al, false);
    f(&mut b, "stackCounterAlignItems", al, false);
    f(&mut b, "stackPrimarySizing", sz, false);
    f(&mut b, "stackCounterSizing", sz, false);
    let (ca, sp) = (def(&b, "StackCounterAlign"), def(&b, "StackPositioning"));
    f(&mut b, "stackChildAlignSelf", ca, false);
    f(&mut b, "stackChildPrimaryGrow", nat(Float), false);
    f(&mut b, "stackPositioning", sp, false);
    let cn = def(&b, "ConstraintType");
    f(&mut b, "horizontalConstraint", cn, false);
    f(&mut b, "verticalConstraint", cn, false);
    let fgt = def(&b, "FillGeometry");
    f(&mut b, "fillGeometry", fgt, true);
    let ad = def(&b, "ArcData");
    f(&mut b, "arcData", ad, false);
    f(&mut b, "count", nat(Uint), false);
    f(&mut b, "starInnerScale", nat(Float), false);
    let bo = def(&b, "BooleanOperation");
    f(&mut b, "booleanOperation", bo, false);
    let sd = b.define("SymbolData", DefKind::Message);
    let gt = def(&b, "GUID");
    b.field(sd, "symbolID", gt, false, 1);
    let sdt = def(&b, "SymbolData");
    f(&mut b, "symbolData", sdt, false);
    let tdt = def(&b, "TextData");
    f(&mut b, "textData", tdt, false);
    let (fnt, nm) = (def(&b, "FontName"), def(&b, "Number"));
    f(&mut b, "fontName", fnt, false);
    f(&mut b, "fontSize", nat(Float), false);
    f(&mut b, "letterSpacing", nm, false);
    f(&mut b, "lineHeight", nm, false);
    let (ah, av, ar, tc, tdc) =
        (def(&b, "TextAlignHorizontal"), def(&b, "TextAlignVertical"), def(&b, "TextAutoResize"), def(&b, "TextCase"), def(&b, "TextDecoration"));
    f(&mut b, "textAlignHorizontal", ah, false);
    f(&mut b, "textAlignVertical", av, false);
    f(&mut b, "textAutoResize", ar, false);
    f(&mut b, "textCase", tc, false);
    f(&mut b, "textDecoration", tdc, false);
    let sidt = def(&b, "StyleId");
    f(&mut b, "styleIdForFill", sidt, false);

    let blob = b.define("Blob", DefKind::Message);
    b.field(blob, "bytes", nat(Byte), true, 1);
    let msg = b.define("Message", DefKind::Message);
    let (nct, bt) = (def(&b, "NodeChange"), def(&b, "Blob"));
    b.field(msg, "nodeChanges", nct, true, 1);
    b.field(msg, "blobs", bt, true, 2);
    b.build()
}

// ---- value builders

pub fn msg(fields: Vec<(&str, V)>) -> V {
    V::Message(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub fn st(fields: Vec<(&str, V)>) -> V {
    V::Struct(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub fn en(n: &str) -> V {
    V::Enum(n.into())
}
pub fn fl(x: f32) -> V {
    V::Float(x)
}
pub fn guid(s: u32, l: u32) -> V {
    st(vec![("sessionID", V::Uint(s)), ("localID", V::Uint(l))])
}
pub fn vec2(x: f32, y: f32) -> V {
    st(vec![("x", fl(x)), ("y", fl(y))])
}
pub fn matrix(tx: f32, ty: f32) -> V {
    st(vec![("m00", fl(1.0)), ("m01", fl(0.0)), ("m02", fl(tx)), ("m10", fl(0.0)), ("m11", fl(1.0)), ("m12", fl(ty))])
}
pub fn color(r: f32, g: f32, b: f32, a: f32) -> V {
    st(vec![("r", fl(r)), ("g", fl(g)), ("b", fl(b)), ("a", fl(a))])
}
pub fn solid(r: f32, g: f32, b: f32) -> V {
    msg(vec![("type", en("SOLID")), ("color", color(r, g, b, 1.0)), ("opacity", fl(1.0)), ("visible", V::Bool(true))])
}
pub fn node(id: (u32, u32), parent: (u32, u32), pos: &str, ty: &str, name: &str, mut extra: Vec<(&str, V)>) -> V {
    let mut f = vec![
        ("guid", guid(id.0, id.1)),
        ("parentIndex", st(vec![("guid", guid(parent.0, parent.1)), ("position", V::String(pos.into()))])),
        ("type", en(ty)),
        ("name", V::String(name.into())),
    ];
    f.append(&mut extra);
    msg(f)
}

/// Commands blob for a closed triangle: M(0,0) L(10,0) L(5,8) Z.
pub fn triangle_blob() -> Vec<u8> {
    let mut b = Vec::new();
    for (op, pts) in [(1u8, &[0.0f32, 0.0][..]), (2, &[10.0, 0.0]), (2, &[5.0, 8.0])] {
        b.push(op);
        for p in pts {
            b.extend(p.to_le_bytes());
        }
    }
    b.push(0);
    b
}

pub const IMAGE_HASH: [u8; 4] = [0xde, 0xad, 0xbe, 0xef];

/// A small but representative document. Siblings are deliberately listed out
/// of z-order to exercise fractional-index sorting.
pub fn sample_message() -> V {
    let doc = node((0, 0), (0, 0), "!", "DOCUMENT", "Document", vec![]);
    let page = node((0, 1), (0, 0), "a", "CANVAS", "Page 1", vec![("backgroundColor", color(0.9, 0.9, 0.9, 1.0))]);
    let internal = node((0, 2), (0, 0), "b", "CANVAS", "Internal Only Canvas", vec![("internalOnly", V::Bool(true))]);
    let frame = node(
        (1, 1),
        (0, 1),
        "a",
        "FRAME",
        "Card",
        vec![
            ("size", vec2(200.0, 300.0)),
            ("transform", matrix(40.0, 50.0)),
            ("fillPaints", V::Array(vec![solid(1.0, 1.0, 1.0)])),
            ("cornerRadius", fl(12.0)),
            ("effects", V::Array(vec![msg(vec![
                ("type", en("DROP_SHADOW")),
                ("color", color(0.0, 0.0, 0.0, 0.25)),
                ("offset", vec2(0.0, 4.0)),
                ("radius", fl(8.0)),
                ("spread", fl(0.0)),
                ("visible", V::Bool(true)),
            ])])),
            ("stackMode", en("VERTICAL")),
            ("stackSpacing", fl(8.0)),
            ("stackHorizontalPadding", fl(16.0)),
            ("stackVerticalPadding", fl(12.0)),
            ("stackPrimarySizing", en("RESIZE_TO_FIT")),
            ("stackCounterAlignItems", en("CENTER")),
        ],
    );
    let rect = node(
        (1, 2),
        (1, 1),
        "b", // listed first, but sorts after the text
        "ROUNDED_RECTANGLE",
        "Image rect",
        vec![
            ("size", vec2(100.0, 40.0)),
            ("transform", matrix(16.0, 12.0)),
            ("fillPaints", V::Array(vec![msg(vec![
                ("type", en("IMAGE")),
                ("image", msg(vec![("hash", V::Bytes(IMAGE_HASH.to_vec()))])),
                ("imageScaleMode", en("FILL")),
            ])])),
            ("strokePaints", V::Array(vec![solid(1.0, 0.0, 0.0)])),
            ("strokeWeight", fl(2.0)),
            ("strokeAlign", en("OUTSIDE")),
            ("stackChildAlignSelf", en("STRETCH")),
            ("blendMode", en("MULTIPLY")),
        ],
    );
    let text = node(
        (1, 3),
        (1, 1),
        "a",
        "TEXT",
        "Title",
        vec![
            ("size", vec2(120.0, 20.0)),
            ("transform", matrix(16.0, 60.0)),
            ("fillPaints", V::Array(vec![solid(0.1, 0.2, 0.3)])),
            ("fontSize", fl(16.0)),
            ("fontName", st(vec![("family", V::String("Inter".into())), ("style", V::String("Medium".into())), ("postscript", V::String("Inter-Medium".into()))])),
            ("letterSpacing", st(vec![("value", fl(10.0)), ("units", en("PERCENT"))])),
            ("lineHeight", st(vec![("value", fl(24.0)), ("units", en("PIXELS"))])),
            ("textAlignHorizontal", en("CENTER")),
            ("textAutoResize", en("WIDTH_AND_HEIGHT")),
            ("textData", msg(vec![
                ("characters", V::String("Hi \u{1F600}!".into())),
                // 'H','i',' ', surrogate pair (2 units), '!' -> UTF-16 length 6
                ("characterStyleIDs", V::Array([0u32, 0, 0, 1, 1, 0].iter().map(|x| V::Uint(*x)).collect())),
                ("styleOverrideTable", V::Array(vec![msg(vec![
                    ("styleID", V::Uint(1)),
                    ("fontName", st(vec![("family", V::String("Inter".into())), ("style", V::String("Bold Italic".into())), ("postscript", V::String("".into()))])),
                    ("fillPaints", V::Array(vec![solid(1.0, 0.0, 0.0)])),
                    ("textDecoration", en("UNDERLINE")),
                ])])),
            ])),
        ],
    );
    let vector = node(
        (1, 4),
        (1, 1),
        "c",
        "VECTOR",
        "Tri",
        vec![
            ("size", vec2(10.0, 8.0)),
            ("transform", matrix(0.0, 100.0)),
            ("fillPaints", V::Array(vec![solid(0.0, 0.0, 0.0)])),
            ("fillGeometry", V::Array(vec![msg(vec![("windingRule", en("NONZERO")), ("commandsBlob", V::Uint(0))])])),
        ],
    );
    let comp = node((1, 5), (1, 1), "d", "SYMBOL", "Button", vec![("size", vec2(80.0, 30.0)), ("transform", matrix(0.0, 120.0)), ("fillPaints", V::Array(vec![solid(0.0, 0.5, 1.0)]))]);
    let comp_child = node((1, 6), (1, 5), "a", "RECTANGLE", "Bg", vec![("size", vec2(80.0, 30.0)), ("fillPaints", V::Array(vec![solid(0.2, 0.2, 0.2)]))]);
    let inst = node(
        (1, 7),
        (1, 1),
        "e",
        "INSTANCE",
        "Button copy",
        vec![("size", vec2(80.0, 30.0)), ("transform", matrix(100.0, 120.0)), ("symbolData", msg(vec![("symbolID", guid(1, 5))]))],
    );
    let ell = node(
        (1, 8),
        (0, 1),
        "b",
        "ELLIPSE",
        "Pie",
        vec![
            ("size", vec2(50.0, 50.0)),
            ("transform", matrix(300.0, 50.0)),
            ("arcData", st(vec![("startingAngle", fl(0.0)), ("endingAngle", fl(3.0)), ("innerRadius", fl(0.5))])),
        ],
    );
    let removed = V::Message(match node((1, 9), (1, 1), "z", "RECTANGLE", "Removed", vec![("phase", en("REMOVED"))]) {
        V::Message(f) => f,
        _ => unreachable!(),
    });
    let star = node((1, 10), (0, 1), "c", "STAR", "Star", vec![("size", vec2(40.0, 40.0)), ("count", V::Uint(6)), ("starInnerScale", fl(0.5))]);
    let sticky = node((1, 11), (0, 1), "d", "STICKY", "Note", vec![("size", vec2(40.0, 40.0))]);

    msg(vec![
        // Child before parent and shuffled positions on purpose.
        ("nodeChanges", V::Array(vec![inst, rect, text, vector, comp_child, comp, frame, ell, removed, star, sticky, internal, page, doc])),
        ("blobs", V::Array(vec![msg(vec![("bytes", V::Bytes(triangle_blob()))])])),
    ])
}

/// Builds a complete `.fig` byte stream from the sample message.
pub fn sample_fig(how: Compression, zipped: bool) -> Vec<u8> {
    let schema = figma_like_schema();
    let root = schema.find("Message").unwrap();
    let body = schema.encode_message(root, &sample_message()).unwrap();
    let canvas = container::write_container(&schema.encode(), &body, how, 101);
    if !zipped {
        return canvas;
    }
    let mut images = BTreeMap::new();
    images.insert(crate::kiwi::hex(&IMAGE_HASH), vec![0x89, b'P', b'N', b'G']);
    container::write_zip(&canvas, &images, Some(&serde_json::json!({"file_name": "Sample file"})))
}
