//! The plugin bridge accepts a bounded static vector subset, never active SVG.
pub fn validate(source: &str) -> Result<(), String> {
    if source.len() > 2 * 1024 * 1024 {
        return Err("Plugin SVG must be smaller than 2 MiB.".into());
    }
    let doc = roxmltree::Document::parse_with_options(source, roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: 20_000 })
        .map_err(|_| "Plugin returned an invalid or overly complex SVG.".to_string())?;
    if doc.root_element().tag_name().name() != "svg" {
        return Err("Plugin content must be an SVG image.".into());
    }
    for node in doc.descendants().filter(|node| node.is_element()) {
        if node.ancestors().count() > 64 {
            return Err("Plugin SVG is nested too deeply.".into());
        }
        if !matches!(
            node.tag_name().name(),
            "svg"
                | "g"
                | "path"
                | "rect"
                | "circle"
                | "ellipse"
                | "line"
                | "polyline"
                | "polygon"
                | "defs"
                | "clipPath"
                | "mask"
                | "linearGradient"
                | "radialGradient"
                | "stop"
                | "title"
                | "desc"
                | "use"
                | "symbol"
        ) {
            return Err("Only static vector icons can be inserted by this plugin runtime.".into());
        }
        for attr in node.attributes() {
            let name = attr.name().to_ascii_lowercase();
            let value = attr.value().trim();
            if name.starts_with("on") || name == "style" || (name == "href" && !local_id(value)) {
                return Err("Plugin SVG cannot contain scripts, styles or external references.".into());
            }
            if value.to_ascii_lowercase().contains("url") && !(value.starts_with("url(") && value.ends_with(')') && local_id(&value[4..value.len() - 1])) {
                return Err("Plugin SVG contains an unsupported resource reference.".into());
            }
        }
    }
    Ok(())
}

fn local_id(value: &str) -> bool {
    value.strip_prefix('#').is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_static_vectors_and_rejects_active_or_external_content() {
        assert!(validate(r##"<svg width="24" height="24"><defs><path id="a" d="M0 0h10v10z"/></defs><use href="#a"/></svg>"##).is_ok());
        for svg in [
            "<svg><script/></svg>",
            "<svg><foreignObject/></svg>",
            "<svg onload='run()'/>",
            "<svg><image href='file:///secret'/></svg>",
            "<svg><use href='https://example.com/icon.svg'/></svg>",
            "<svg><path fill='url(https://example.com)'/></svg>",
            "<!DOCTYPE svg [<!ENTITY x 'entity'>]><svg>&x;</svg>",
        ] {
            assert!(validate(svg).is_err(), "{svg}");
        }
    }
}
