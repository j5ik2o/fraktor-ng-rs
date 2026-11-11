use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// One `.rs` file: its path, its text, and the syn parse result.
///
/// The path is kept as it was discovered, relative to the root given on the
/// command line. Path-based predicates must see that form, not an absolute
/// path, so that a parent directory named `tests` or `target` does not change
/// the result.
pub struct SourceFile {
    path: PathBuf,
    text: String,
    ast: Result<syn::File, String>,
}

impl SourceFile {
    pub fn load(path: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        Ok(Self::from_text(path, text))
    }

    pub fn from_text(path: &Path, text: String) -> Self {
        let ast = syn::parse_file(&text).map_err(|error| {
            let start = error.span().start();
            format!("{}:{}: {error}", start.line, start.column + 1)
        });
        Self {
            path: path.to_path_buf(),
            text,
            ast,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The parsed file, or `None` when syn could not parse it.
    pub fn ast(&self) -> Option<&syn::File> {
        self.ast.as_ref().ok()
    }

    pub fn parse_error(&self) -> Option<&str> {
        self.ast.as_ref().err().map(String::as_str)
    }

    /// The file name without directories, or an empty string.
    pub fn file_name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
    }

    /// The path with `/` separators, for substring predicates.
    pub fn normalized_path(&self) -> String {
        self.path.to_string_lossy().replace('\\', "/")
    }
}
