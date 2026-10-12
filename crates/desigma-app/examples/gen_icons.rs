//! Regenerates every app-icon asset under `assets/icon/` from the two SVG masters.
//!
//!   cargo run -p desigma-app --example gen_icons
//!
//! Outputs (all committed, so normal builds never need this):
//!   png/icon_<n>.png      full-bleed mark, n = 16..1024 (Linux hicolor, window icon)
//!   desigma.ico           Windows icon, PNG-compressed entries 16/24/32/48/64/256
//!   desigma.iconset/      macOS iconset (padded tile), input for `iconutil -c icns`
//!   desigma.icns          macOS icon, written directly (PNG payloads)
use resvg::{tiny_skia, usvg};
use std::path::{Path, PathBuf};

fn render(svg: &str, px: u32) -> Vec<u8> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).expect("parse icon svg");
    let mut pm = tiny_skia::Pixmap::new(px, px).unwrap();
    let s = px as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(s, s), &mut pm.as_mut());
    pm.encode_png().expect("encode png")
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    println!("wrote {}", path.display());
}

fn ico(entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * entries.len() as u32;
    for (px, png) in entries {
        let b = if *px >= 256 { 0 } else { *px as u8 };
        out.extend_from_slice(&[b, b, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bit depth
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in entries {
        out.extend_from_slice(png);
    }
    out
}

fn icns(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let total: usize = 8 + chunks.iter().map(|(_, d)| 8 + d.len()).sum::<usize>();
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&(total as u32).to_be_bytes());
    for (ty, data) in chunks {
        out.extend_from_slice(*ty);
        out.extend_from_slice(&((8 + data.len()) as u32).to_be_bytes());
        out.extend_from_slice(data);
    }
    out
}

fn main() {
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icon");
    let full = std::fs::read_to_string(root.join("desigma-icon.svg")).unwrap();
    let mac = std::fs::read_to_string(root.join("desigma-icon-macos.svg")).unwrap();

    for px in [16, 32, 48, 64, 128, 256, 512, 1024] {
        write(&root.join(format!("png/icon_{px}.png")), &render(&full, px));
    }

    let entries: Vec<(u32, Vec<u8>)> = [16, 24, 32, 48, 64, 256].iter().map(|&p| (p, render(&full, p))).collect();
    write(&root.join("desigma.ico"), &ico(&entries));

    // (file name, pixels). Apple's iconset naming: icon_<n>x<n>[@2x].png
    let set: [(&str, u32); 10] = [
        ("icon_16x16.png", 16),
        ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32),
        ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128),
        ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256),
        ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512),
        ("icon_512x512@2x.png", 1024),
    ];
    let mut rendered = std::collections::BTreeMap::new();
    for (name, px) in set {
        let png = rendered.entry(px).or_insert_with(|| render(&mac, px)).clone();
        write(&root.join("desigma.iconset").join(name), &png);
    }
    // icns type codes for PNG payloads: ic11=16@2x, ic12=32@2x, ic07=128, ic13=128@2x,
    // ic08=256, ic14=256@2x, ic09=512, ic10=512@2x(1024).
    let chunks: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"ic11", rendered[&32].clone()),
        (b"ic12", rendered[&64].clone()),
        (b"ic07", rendered[&128].clone()),
        (b"ic13", rendered[&256].clone()),
        (b"ic08", rendered[&256].clone()),
        (b"ic14", rendered[&512].clone()),
        (b"ic09", rendered[&512].clone()),
        (b"ic10", rendered[&1024].clone()),
    ];
    write(&root.join("desigma.icns"), &icns(&chunks));
}
