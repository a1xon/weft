use std::{io::Write, path::Path, process::Command};

fn main() {
    // A stamp for this build. `weft --version` shows it, and a running daemon is compared against
    // it: typing `weft` with a freshly installed binary replaces the daemon of an older build.
    let stamp = Command::new("date")
        .args(["-u", "+%Y-%m-%d %H:%M:%S UTC"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=WEFT_BUILD={stamp}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=web");
    // PDF.js for the extension's PDF viewer is most of the extension's size: it goes into the
    // binary gzipped and `weft extension` unpacks it (`browser::PACKED`), and so does markdown-it
    // for its Markdown viewer
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    for (dir, name) in [("pdfjs", "pdf.min.mjs"), ("pdfjs", "pdf.worker.min.mjs"), ("mdit", "markdown-it.min.mjs")] {
        let src = Path::new("extension").join(dir).join(name);
        println!("cargo:rerun-if-changed={}", src.display());
        let bytes = std::fs::read(&src).unwrap_or_else(|e| panic!("{}: {e}", src.display()));
        let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        z.write_all(&bytes).expect("gzip");
        std::fs::write(Path::new(&out).join(format!("{name}.gz")), z.finish().expect("gzip")).expect("OUT_DIR");
    }
}
