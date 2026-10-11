//! Command line: `desigma [file] [--screenshot out.png] [--size WxH] [--select name|none] [--menu name]`.

use std::path::PathBuf;

#[derive(Debug, Default, PartialEq)]
pub struct Options {
    pub file: Option<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub size: Option<(u32, u32)>,
    /// Select the first layer with this name after loading (handy for screenshots).
    pub select: Option<String>,
    /// Open a menu after loading: `main`, `main/File`, `shape`, `palette` (for screenshots).
    pub menu: Option<String>,
    /// Built-in test document instead of the demo (`no-selection`: the Figma page-level reference).
    pub fixture: Option<String>,
    pub help: bool,
}

pub fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once(['x', 'X'])?;
    let (w, h) = (w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?);
    (w >= 200 && h >= 200 && w <= 8192 && h <= 8192).then_some((w, h))
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => o.screenshot = Some(it.next().ok_or("--screenshot needs an output path")?.into()),
            "--size" => {
                let v = it.next().ok_or("--size needs WxH")?;
                o.size = Some(parse_size(&v).ok_or_else(|| format!("invalid size '{v}' (expected WxH, e.g. 1600x1088)"))?);
            }
            "--select" => o.select = Some(it.next().ok_or("--select needs a layer name")?),
            "--menu" => o.menu = Some(it.next().ok_or("--menu needs a name (main, main/File, shape, palette)")?),
            "--fixture" => o.fixture = Some(it.next().ok_or("--fixture needs a name (no-selection)")?),
            "-h" | "--help" => o.help = true,
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            _ => {
                if o.file.is_some() {
                    return Err("only one file can be opened".into());
                }
                o.file = Some(a.into());
            }
        }
    }
    Ok(o)
}

pub const USAGE: &str = "usage: desigma [FILE] [--screenshot OUT.png] [--size WxH] [--select LAYER|none] [--menu main|main/File|shape|palette]\n  FILE  .fig, .json (Figma REST or native) or .desigma";

#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Options, String> {
        parse(a.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_file_and_screenshot() {
        let o = p(&["a.fig", "--screenshot", "out/app.png", "--size", "1600x1088", "--select", "Card"]).unwrap();
        assert_eq!(o.file, Some("a.fig".into()));
        assert_eq!(o.screenshot, Some("out/app.png".into()));
        assert_eq!(o.size, Some((1600, 1088)));
        assert_eq!(o.select.as_deref(), Some("Card"));
    }

    #[test]
    fn parses_menu_and_select_none() {
        let o = p(&["--menu", "main/File", "--select", "none"]).unwrap();
        assert_eq!(o.menu.as_deref(), Some("main/File"));
        assert_eq!(o.select.as_deref(), Some("none"));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(p(&["--size", "huge"]).is_err());
        assert!(p(&["--screenshot"]).is_err());
        assert!(p(&["--bogus"]).is_err());
        assert!(p(&["--menu"]).is_err());
        assert!(p(&["a", "b"]).is_err());
        assert_eq!(parse_size("1440X900"), Some((1440, 900)));
        assert_eq!(parse_size("10x10"), None);
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(p(&[]).unwrap(), Options::default());
    }
}
