//! Imports a `.fig` file, prints import statistics and renders the top-level
//! frames of one page to PNGs.
//!
//!   cargo run --release -p desigma-fig --example fig_render -- file.fig [--page <name substring>] [--out out/fig-check] [--max <frames>]
//!
//! Without `--page` the page with the most nodes is used. The renders are local
//! checks only: never commit them.

use desigma_doc::*;
use std::time::Instant;

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let t = out.trim_matches('-');
    if t.is_empty() {
        "frame".into()
    } else {
        t.chars().take(60).collect()
    }
}

fn count(doc: &Document, root: NodeId) -> usize {
    let mut n = 0;
    doc.walk(root, |_, _| n += 1);
    n
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: fig_render <file.fig> [--page <substr>] [--out <dir>] [--max <n>]");
        std::process::exit(2);
    };
    let (mut page_q, mut out_dir, mut max) = (None::<String>, "out/fig-check".to_string(), usize::MAX);
    let (mut node_q, mut scale_arg) = (None::<String>, None::<f32>);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--page" => page_q = args.next(),
            "--out" => out_dir = args.next().unwrap_or(out_dir),
            "--max" => max = args.next().and_then(|s| s.parse().ok()).unwrap_or(max),
            "--node" => node_q = args.next(),
            "--scale" => scale_arg = args.next().and_then(|s| s.parse().ok()),
            _ => {}
        }
    }
    let bytes = std::fs::read(&path).expect("read file");
    let t = Instant::now();
    let doc = desigma_fig::import_fig(&bytes).expect("import");
    println!("import: {:.2}s ({} bytes)", t.elapsed().as_secs_f64(), bytes.len());

    let mut unknown = 0;
    let mut instances = 0;
    let mut resolved = 0;
    let mut empty = 0;
    for n in doc.nodes.values() {
        match &n.kind {
            NodeKind::Unknown { .. } => unknown += 1,
            NodeKind::Instance { component, .. } => {
                instances += 1;
                resolved += component.is_some() as usize;
                empty += n.children.is_empty() as usize;
            }
            _ => {}
        }
    }
    println!("nodes total {} | unknown {unknown} | instances {instances} resolved {resolved} childless {empty}", doc.nodes.len());
    println!(
        "variables {} collections {} | styles {} | external assets {} | library nodes {} | images {}",
        doc.variables.len(),
        doc.variable_collections.len(),
        doc.styles.len(),
        doc.library.external.len(),
        doc.library_page.map(|p| count(&doc, p)).unwrap_or(0),
        doc.images.len()
    );
    for &p in &doc.pages {
        println!("page {:?}: {} nodes", doc.get(p).map(|n| n.name.as_str()).unwrap_or(""), count(&doc, p));
    }

    let page = match &page_q {
        Some(q) => doc.pages.iter().copied().find(|p| doc.get(*p).is_some_and(|n| n.name.contains(q.as_str()))),
        None => doc.pages.iter().copied().max_by_key(|p| count(&doc, *p)),
    };
    let Some(page) = page else { return };
    std::fs::create_dir_all(&out_dir).expect("create out dir");
    let mut used = std::collections::BTreeSet::new();
    // `--node <substr>`: render the first node (anywhere on the page) whose name contains it.
    let tops: Vec<NodeId> = match &node_q {
        Some(q) => {
            let mut hit = None;
            doc.walk(page, |n, _| {
                if hit.is_none() && n.name.contains(q.as_str()) && n.size.x > 0.0 {
                    hit = Some(n.id);
                }
            });
            hit.into_iter().collect()
        }
        None => doc.get(page).map(|p| p.children.clone()).unwrap_or_default(),
    };
    for (i, &c) in tops.iter().enumerate().take(max) {
        let Some(n) = doc.get(c) else { continue };
        if !n.visible || n.size.x < 1.0 || n.size.y < 1.0 {
            continue;
        }
        if node_q.is_some() {
            // Debug aid: the subtree's structure (kind, size, transform, clip, fills, geometry).
            doc.walk(c, |n, depth| {
                let t = n.transform;
                println!(
                    "{}{:?} {:?} size=({},{}) xf=[{} {} {} {} {} {}] vis={} fills={} fillgeo={} strokegeo={} clip={:?}",
                    "  ".repeat(depth),
                    std::mem::discriminant(&n.kind),
                    n.name,
                    n.size.x,
                    n.size.y,
                    t.a,
                    t.b,
                    t.c,
                    t.d,
                    t.tx,
                    t.ty,
                    n.visible,
                    n.fills.len(),
                    n.fill_geometry.as_ref().map(|g| g.len()).unwrap_or(0),
                    n.stroke_geometry.as_ref().map(|g| g.len()).unwrap_or(0),
                    n.kind.frame_props().map(|f| f.clips_content)
                );
            });
        }
        let scale = scale_arg.unwrap_or_else(|| (2400.0 / n.size.x.max(n.size.y)).min(1.0));
        let t = Instant::now();
        let Some(pm) = desigma_render::render_node(&doc, c, scale) else { continue };
        let mut name = slug(&n.name);
        if !used.insert(name.clone()) {
            name = format!("{name}-{i}");
        }
        let file = format!("{out_dir}/{name}.png");
        pm.save_png(&file).expect("write png");
        println!("rendered {file}: {}x{} ({} nodes) in {:.2}s", pm.width(), pm.height(), count(&doc, c), t.elapsed().as_secs_f64());
    }
}
