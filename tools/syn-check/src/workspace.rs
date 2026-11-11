use std::{
    cell::RefCell,
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::{run_error::RunError, source_file::SourceFile};

/// Loads and caches source files, so that a rule can look at a file other
/// than the one being checked (for example a child module).
#[derive(Default)]
pub struct Workspace {
    cache: RefCell<HashMap<PathBuf, Option<Rc<SourceFile>>>>,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    /// The file at `path`, or `None` when it cannot be read.
    pub fn source(&self, path: &Path) -> Option<Rc<SourceFile>> {
        if let Some(cached) = self.cache.borrow().get(path) {
            return cached.clone();
        }
        let loaded = SourceFile::load(path).ok().map(Rc::new);
        self.cache
            .borrow_mut()
            .insert(path.to_path_buf(), loaded.clone());
        loaded
    }

    /// Every `.rs` file under the roots, sorted. A root may be a file.
    ///
    /// `target`, and directories whose name starts with a dot, are skipped.
    pub fn discover(roots: &[PathBuf]) -> Result<Vec<PathBuf>, RunError> {
        let mut files = Vec::new();
        for root in roots {
            let io_error = |error| RunError::Io(root.clone(), error);
            if fs::metadata(root).map_err(io_error)?.is_dir() {
                collect(root, &mut files).map_err(io_error)?;
            } else {
                files.push(root.clone());
            }
        }
        files.sort();
        files.dedup();
        Ok(files)
    }
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}
