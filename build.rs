//! Packs `assets/` into the binary: KaTeX, highlight.js and Mermaid, so notes
//! render offline. Text files are compressed here with Brotli and served that
//! way; WebKit decodes them. Brotli shrinks Mermaid a quarter more than gzip.
//! WebKit only advertises gzip over plain HTTP, but decodes Brotli all the same.
//! Fonts are already compressed and go in as they are.
//! The note editor's own code lives in `src/page/` but is packed the same way, as
//! `visor/…`: it only loads when someone starts editing.

use std::fs;
use std::path::{Path, PathBuf};

const EDITOR: [&str; 2] = ["editor.js", "editor.css"];

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let mut files = vec![];
    collect(Path::new("assets"), &mut files);
    files.sort();
    let mut named: Vec<(String, PathBuf)> = files
        .into_iter()
        .map(|path| (path.strip_prefix("assets").unwrap().to_string_lossy().replace('\\', "/"), path))
        .collect();
    for file in EDITOR {
        named.push((format!("visor/{file}"), Path::new("src/page").join(file)));
        println!("cargo:rerun-if-changed=src/page/{file}");
    }

    let mut table = String::from("pub static ASSETS: &[(&str, &[u8], bool)] = &[\n");
    for (i, (name, path)) in named.iter().enumerate() {
        let bytes = fs::read(path).unwrap();
        let compressed = !name.ends_with(".woff2");
        let stored = if compressed {
            let mut out = vec![];
            let params = brotli::enc::BrotliEncoderParams { quality: 11, lgwin: 24, ..Default::default() };
            brotli::BrotliCompress(&mut bytes.as_slice(), &mut out, &params).unwrap();
            out
        } else {
            bytes
        };
        let file = out.join(format!("asset{i}"));
        fs::write(&file, stored).unwrap();
        table += &format!("    ({name:?}, include_bytes!({:?}), {compressed}),\n", file.display().to_string());
    }
    table += "];\n";
    fs::write(out.join("assets.rs"), table).unwrap();
    println!("cargo:rerun-if-changed=assets");
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}
