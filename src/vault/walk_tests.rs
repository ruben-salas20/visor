use super::*;
use std::os::unix::fs::symlink;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("visor-walk-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn listed(dir: &Dir, out: &mut Vec<String>) {
    out.extend(dir.files.iter().filter(|f| f.kind == Kind::Page).map(|f| f.path.clone()));
    dir.dirs.iter().for_each(|d| listed(d, out));
}

#[test]
fn panel_lists_pages_images_other_files_and_empty_folders() {
    let dir = temp("panel");
    let root = dir.join("v");
    fs::create_dir_all(root.join("vacia")).unwrap();
    fs::create_dir_all(root.join("con")).unwrap();
    for f in ["con/nota.md", "con/FOTO.PNG", "con/tabla.xlsx", "con/Libro.PDF", "con/.oculto.png"] {
        fs::write(root.join(f), "").unwrap();
    }
    let top = tree(&root);
    let con = top.dirs.iter().find(|d| d.name == "con").unwrap();
    let empty = top.dirs.iter().find(|d| d.name == "vacia").unwrap();
    assert!(empty.dirs.is_empty() && empty.files.is_empty());
    let got: Vec<_> = con.files.iter().map(|f| (f.name.as_str(), f.path.as_str(), f.kind)).collect();
    assert_eq!(
        got,
        [
            ("FOTO.PNG", "con/FOTO.PNG", Kind::Image),
            ("Libro.PDF", "con/Libro.PDF", Kind::Pdf),
            ("nota", "con/nota.md", Kind::Page),
            ("tabla.xlsx", "con/tabla.xlsx", Kind::Other),
        ]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn link_loops_are_walked_once() {
    let dir = temp("loop");
    let root = dir.join("v");
    fs::create_dir_all(root.join("a")).unwrap();
    fs::write(root.join("a/n.md"), "").unwrap();
    for i in 0..4 {
        symlink(".", root.join(format!("bucle{i}"))).unwrap();
        symlink("..", root.join(format!("a/sube{i}"))).unwrap();
    }
    let start = std::time::Instant::now();
    let mut from_tree = vec![];
    listed(&tree(&root), &mut from_tree);
    let flat: Vec<_> = files(&root, &root, is_page).iter().map(|p| rel(&root, p)).collect();
    assert_eq!(from_tree, ["a/n.md"]);
    assert_eq!(flat, ["a/n.md"]);
    assert!(has_pages(&root));
    assert!(start.elapsed().as_secs() < 2);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn links_out_of_the_vault_are_not_followed() {
    let dir = temp("out");
    let root = dir.join("v");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(dir.join("fuera")).unwrap();
    fs::write(dir.join("fuera/x.md"), "").unwrap();
    symlink(dir.join("fuera"), root.join("salida")).unwrap();
    symlink(&dir, root.join("padre")).unwrap();
    let mut found = vec![];
    listed(&tree(&root), &mut found);
    assert!(found.is_empty(), "{found:?}");
    assert!(files(&root, &root, is_page).is_empty());
    assert!(!has_pages(&root));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn one_policy_for_hidden_names_and_duplicate_links() {
    let dir = temp("hidden");
    let root = dir.join("v");
    for d in ["real", ".oculta", "node_modules/p"] {
        fs::create_dir_all(root.join(d)).unwrap();
    }
    for f in ["real/n.md", ".oculta/x.md", "node_modules/p/y.md", "raiz.md", ".dot.md"] {
        fs::write(root.join(f), "").unwrap();
    }
    symlink("real", root.join("alias")).unwrap();
    let mut found = vec![];
    listed(&tree(&root), &mut found);
    found.sort();
    assert_eq!(found, ["raiz.md", "real/n.md"], "the real folder wins over its link");
    let mut flat: Vec<_> = files(&root, &root, is_page).iter().map(|p| rel(&root, p)).collect();
    flat.sort();
    assert_eq!(flat, found);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pdf_is_not_other_in_the_tree() {
    let dir = temp("pdf");
    let root = dir.join("v");
    fs::create_dir_all(root.join("libros")).unwrap();
    fs::write(root.join("libros/manual.pdf"), "%PDF-1.4").unwrap();
    let top = tree(&root);
    let libros = top.dirs.iter().find(|d| d.name == "libros").unwrap();
    assert_eq!(libros.files.iter().map(|f| f.kind).collect::<Vec<_>>(), [Kind::Pdf]);
    assert_eq!(libros.files[0].name, "manual.pdf", "a PDF keeps its extension, as images do");
    assert!(!is_page(&root.join("libros/manual.pdf")));
    fs::remove_dir_all(dir).unwrap();
}
