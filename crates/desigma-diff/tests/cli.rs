use std::path::{Path, PathBuf};
use std::process::Command;

use image::{Rgba, RgbaImage};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_desigma-diff"))
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("desigma-diff-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn save(dir: &Path, name: &str, img: &RgbaImage) -> PathBuf {
    let p = dir.join(name);
    img.save(&p).unwrap();
    p
}

fn white() -> RgbaImage {
    RgbaImage::from_pixel(16, 16, Rgba([255, 255, 255, 255]))
}

fn off() -> RgbaImage {
    let mut b = white();
    b.put_pixel(5, 5, Rgba([0, 0, 0, 255]));
    b
}

#[test]
fn compare_pass_fail_json_and_ignore() {
    let d = tmp("compare");
    let a = save(&d, "a.png", &white());
    let b = save(&d, "b.png", &off());
    let ok = bin().arg("compare").arg(&a).arg(&a).output().unwrap();
    assert!(ok.status.success());
    assert!(String::from_utf8_lossy(&ok.stdout).starts_with("PASS"));

    let diff = d.join("diff.png");
    let bad = bin().arg("compare").arg(&a).arg(&b).arg("--out").arg(&diff).output().unwrap();
    assert_eq!(bad.status.code(), Some(1));
    assert!(diff.exists());

    let lenient = bin().arg("compare").arg(&a).arg(&b).args(["--threshold", "0.01"]).output().unwrap();
    assert!(lenient.status.success());

    let ign = bin().arg("compare").arg(&a).arg(&b).args(["--ignore", "4,4,3,3"]).output().unwrap();
    assert!(ign.status.success());

    let json = bin().arg("compare").arg(&a).arg(&b).arg("--json").output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(v["mismatched"], 1);
    assert_eq!(v["pass"], false);
}

#[test]
fn suite_writes_report_and_fails() {
    let d = tmp("suite");
    save(&d, "ref_ok.png", &white());
    save(&d, "act_ok.png", &white());
    save(&d, "ref_bad.png", &white());
    save(&d, "act_bad.png", &off());
    std::fs::write(
        d.join("manifest.json"),
        r#"{"cases":[
            {"name":"good","reference":"ref_ok.png","actual":"act_ok.png","threshold":0.0},
            {"name":"bad one","reference":"ref_bad.png","actual":"act_bad.png","threshold":0.0}
        ]}"#,
    )
    .unwrap();
    let out = bin().arg("suite").arg(d.join("manifest.json")).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("FAIL") && stdout.contains("PASS"));
    let html = std::fs::read_to_string(d.join("diff-report/index.html")).unwrap();
    assert!(html.find("bad one").unwrap() < html.find("good").unwrap(), "worst first");

    // Passing manifest with a region that excludes the differing pixel.
    std::fs::write(
        d.join("ok.json"),
        r#"[{"name":"cropped","reference":"ref_bad.png","actual":"act_bad.png","threshold":0.0,
             "region":{"x":0,"y":0,"w":4,"h":4}}]"#,
    )
    .unwrap();
    let out = bin().arg("suite").arg(d.join("ok.json")).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}
