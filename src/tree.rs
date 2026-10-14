//! The folders and files of a set of paths, as the rows a tree view shows.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub struct Tree {
    folders: BTreeMap<String, Tree>,
    files: BTreeSet<String>,
}

#[derive(Debug)]
pub struct Row {
    pub depth: usize,
    pub name: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder { open: bool },
    File { current: bool },
}

impl Tree {
    pub fn new<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        let mut root = Tree::default();
        for path in paths {
            let (folders, file) = path.rsplit_once('/').unwrap_or(("", path));
            let mut folder = &mut root;
            for part in folders.split('/').filter(|part| !part.is_empty()) {
                folder = folder.folders.entry(part.to_string()).or_default();
            }
            folder.files.insert(file.to_string());
        }
        root
    }

    /// Folders above files at each level. Only the folders on the way to
    /// `current` are open, so the rows are the levels along one path and
    /// not the whole tree.
    pub fn rows(&self, current: &str) -> Vec<Row> {
        let parts: Vec<&str> = current.split('/').collect();
        let mut rows = Vec::new();
        self.push_rows(&parts, 0, &mut rows);
        rows
    }

    fn push_rows(&self, path: &[&str], depth: usize, rows: &mut Vec<Row>) {
        let (head, rest) = path.split_first().map_or(("", &[][..]), |(h, r)| (*h, r));
        for (name, folder) in &self.folders {
            let open = !rest.is_empty() && name == head;
            rows.push(Row {
                depth,
                name: name.clone(),
                kind: Kind::Folder { open },
            });
            if open {
                folder.push_rows(rest, depth + 1, rows);
            }
        }
        for name in &self.files {
            let current = rest.is_empty() && name == head;
            rows.push(Row {
                depth,
                name: name.clone(),
                kind: Kind::File { current },
            });
        }
    }
}
