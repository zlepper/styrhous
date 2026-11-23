//! A small Rust lexer shared by the source guards (`colour_guard`,
//! `glyph_guard`): it separates code from comments and string literals so a
//! guard neither misses code after a `//` inside a string nor matches text in
//! comments.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// A string, byte string or char literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Literal {
    /// 1-based line the literal starts on.
    pub line: usize,
    /// The literal's value with escapes decoded.
    pub value: String,
}

/// A lexed source file.
#[derive(Clone, Debug, Default)]
pub struct Lexed {
    /// The source with comments and the contents of string and char
    /// literals replaced by spaces. Newlines are kept, so line numbers match.
    pub code: String,
    /// Every string and char literal, in source order.
    pub literals: Vec<Literal>,
}

/// One Rust source file of the workspace.
pub struct SourceFile {
    /// Path relative to the workspace root, with `/` separators.
    pub relative: String,
    pub path: PathBuf,
}

/// The workspace root (the parent of `crates/`).
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Every `.rs` file under `crates/*/src`, sorted.
pub fn workspace_sources() -> Vec<SourceFile> {
    let root = workspace_root();
    let mut crate_dirs: Vec<_> = std::fs::read_dir(root.join("crates"))
        .expect("crates directory")
        .map(|entry| entry.expect("crate entry").path())
        .collect();
    crate_dirs.sort();
    let mut paths = Vec::new();
    for dir in crate_dirs {
        let src = dir.join("src");
        if src.is_dir() {
            rust_files(&src, &mut paths);
        }
    }
    assert!(
        !paths.is_empty(),
        "found no sources under {}",
        root.display()
    );
    paths
        .into_iter()
        .map(|path| SourceFile {
            relative: path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/"),
            path,
        })
        .collect()
}

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Lex `source` into code and literals.
pub fn lex(source: &str) -> Lexed {
    let chars: Vec<char> = source.chars().collect();
    let mut lexer = Lexer {
        chars: &chars,
        index: 0,
        line: 1,
        lexed: Lexed::default(),
    };
    lexer.run();
    lexer.lexed
}

struct Lexer<'a> {
    chars: &'a [char],
    index: usize,
    line: usize,
    lexed: Lexed,
}

impl Lexer<'_> {
    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.index + offset).copied()
    }

    /// Copy the current character to the code.
    fn keep(&mut self) {
        let c = self.chars[self.index];
        if c == '\n' {
            self.line += 1;
        }
        self.lexed.code.push(c);
        self.index += 1;
    }

    /// Blank the current character out of the code, keeping newlines.
    fn blank(&mut self) {
        let c = self.chars[self.index];
        if c == '\n' {
            self.line += 1;
            self.lexed.code.push('\n');
        } else {
            self.lexed.code.push(' ');
        }
        self.index += 1;
    }

    fn run(&mut self) {
        while let Some(c) = self.peek(0) {
            match c {
                '/' if self.peek(1) == Some('/') => {
                    while self.peek(0).is_some_and(|c| c != '\n') {
                        self.blank();
                    }
                }
                '/' if self.peek(1) == Some('*') => self.block_comment(),
                '"' => self.string(false),
                '\'' => self.char_or_lifetime(),
                c if is_ident(c) => self.identifier(),
                _ => self.keep(),
            }
        }
    }

    fn block_comment(&mut self) {
        let mut depth = 0;
        while let Some(c) = self.peek(0) {
            if c == '/' && self.peek(1) == Some('*') {
                depth += 1;
                self.blank();
                self.blank();
            } else if c == '*' && self.peek(1) == Some('/') {
                depth -= 1;
                self.blank();
                self.blank();
                if depth == 0 {
                    return;
                }
            } else {
                self.blank();
            }
        }
    }

    fn identifier(&mut self) {
        let start = self.index;
        while self.peek(0).is_some_and(is_ident) {
            self.keep();
        }
        let word: String = self.chars[start..self.index].iter().collect();
        match (word.as_str(), self.peek(0)) {
            ("r" | "br" | "cr", Some('"' | '#')) => self.raw_string(),
            ("b" | "c", Some('"')) => self.string(false),
            ("b", Some('\'')) => self.string(true),
            _ => {}
        }
    }

    /// A raw string starting at its `#`s or opening quote; leaves raw
    /// identifiers (`r#type`) alone.
    fn raw_string(&mut self) {
        let mut hashes = 0;
        while self.peek(hashes) == Some('#') {
            hashes += 1;
        }
        if self.peek(hashes) != Some('"') {
            return;
        }
        for _ in 0..=hashes {
            self.keep();
        }
        let line = self.line;
        let mut value = String::new();
        loop {
            match self.peek(0) {
                None => break,
                Some('"') if (1..=hashes).all(|offset| self.peek(offset) == Some('#')) => {
                    for _ in 0..=hashes {
                        self.keep();
                    }
                    break;
                }
                Some(c) => {
                    value.push(c);
                    self.blank();
                }
            }
        }
        self.lexed.literals.push(Literal { line, value });
    }

    /// A string (or, with `char`, a char) literal starting at its quote.
    fn string(&mut self, char: bool) {
        let quote = if char { '\'' } else { '"' };
        let line = self.line;
        self.keep();
        let mut value = String::new();
        while let Some(c) = self.peek(0) {
            if c == quote {
                self.keep();
                break;
            }
            if c == '\\' {
                self.escape(&mut value);
            } else {
                value.push(c);
                self.blank();
            }
        }
        self.lexed.literals.push(Literal { line, value });
    }

    fn escape(&mut self, value: &mut String) {
        self.blank();
        let Some(c) = self.peek(0) else {
            return;
        };
        self.blank();
        match c {
            'n' => value.push('\n'),
            't' => value.push('\t'),
            'r' => value.push('\r'),
            '0' => value.push('\0'),
            'x' => {
                let digits: String = (0..2).filter_map(|offset| self.peek(offset)).collect();
                for _ in 0..digits.chars().count() {
                    self.blank();
                }
                if let Ok(code) = u8::from_str_radix(&digits, 16) {
                    value.push(char::from(code));
                }
            }
            'u' => {
                let mut digits = String::new();
                while let Some(c) = self.peek(0) {
                    self.blank();
                    match c {
                        '{' => {}
                        '}' => break,
                        c => digits.push(c),
                    }
                }
                let decoded = u32::from_str_radix(&digits.replace('_', ""), 16)
                    .ok()
                    .and_then(char::from_u32);
                if let Some(decoded) = decoded {
                    value.push(decoded);
                }
            }
            '\n' => {
                while self.peek(0).is_some_and(char::is_whitespace) {
                    self.blank();
                }
            }
            other => value.push(other),
        }
    }

    fn char_or_lifetime(&mut self) {
        let is_char = self.peek(1) == Some('\\') || self.peek(2) == Some('\'');
        if is_char {
            self.string(true);
        } else {
            self.keep();
        }
    }
}

/// Line ranges (1-based, inclusive) of `#[cfg(test)] mod name { … }` blocks.
pub fn test_module_lines(code: &str) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = code.lines().collect();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != "#[cfg(test)]" {
            index += 1;
            continue;
        }
        let Some(module) = (index + 1..lines.len()).find(|&next| !lines[next].trim().is_empty())
        else {
            break;
        };
        let header = lines[module].trim_start();
        if !(header.starts_with("mod ") || header.starts_with("pub mod ")) || !header.contains('{')
        {
            index = module;
            continue;
        }
        let mut depth = 0_i32;
        let mut end = module;
        for (offset, line) in lines[module..].iter().enumerate() {
            depth += line.matches('{').count() as i32;
            depth -= line.matches('}').count() as i32;
            if depth == 0 {
                end = module + offset;
                break;
            }
        }
        ranges.push((index + 1, end + 1));
        index = end + 1;
    }
    ranges
}

/// Names of the `#[cfg(test)] mod name;` declarations in `code`, whose
/// bodies live in their own files.
pub fn test_module_files(code: &str) -> Vec<String> {
    let lines: Vec<&str> = code.lines().map(str::trim).collect();
    lines
        .windows(2)
        .filter(|pair| pair[0] == "#[cfg(test)]")
        .filter_map(|pair| {
            pair[1]
                .strip_prefix("mod ")
                .or_else(|| pair[1].strip_prefix("pub mod "))
                .and_then(|rest| rest.strip_suffix(';'))
                .map(str::to_owned)
        })
        .collect()
}

/// Where the file of module `name` declared in `parent` lives.
pub fn module_file(parent: &Path, name: &str) -> [PathBuf; 2] {
    let file_name = parent.file_name().and_then(|name| name.to_str());
    let dir = if matches!(file_name, Some("mod.rs" | "lib.rs" | "main.rs")) {
        parent.parent().expect("module directory").to_path_buf()
    } else {
        parent.with_extension("")
    };
    [
        dir.join(format!("{name}.rs")),
        dir.join(name).join("mod.rs"),
    ]
}

/// Names of every `mod name;` declaration in `code`.
pub fn module_declarations(code: &str) -> Vec<String> {
    code.lines()
        .map(str::trim)
        .filter_map(|line| {
            line.strip_prefix("pub(crate) mod ")
                .or_else(|| line.strip_prefix("pub(super) mod "))
                .or_else(|| line.strip_prefix("pub mod "))
                .or_else(|| line.strip_prefix("mod "))
                .and_then(|rest| rest.strip_suffix(';'))
                .map(str::to_owned)
        })
        .collect()
}

/// The files (relative paths) holding test code: modules declared with
/// `#[cfg(test)] mod name;` and every module declared inside those.
pub fn test_files(files: &[(String, Lexed)]) -> std::collections::BTreeSet<String> {
    let normalise = |path: PathBuf| path.to_string_lossy().replace('\\', "/");
    let mut found: std::collections::BTreeSet<String> = files
        .iter()
        .flat_map(|(relative, lexed)| {
            test_module_files(&lexed.code)
                .into_iter()
                .flat_map(move |name| module_file(Path::new(relative), &name))
        })
        .map(normalise)
        .collect();
    loop {
        let children: Vec<String> = files
            .iter()
            .filter(|(relative, _)| found.contains(relative))
            .flat_map(|(relative, lexed)| {
                module_declarations(&lexed.code)
                    .into_iter()
                    .flat_map(move |name| module_file(Path::new(relative), &name))
            })
            .map(normalise)
            .filter(|path| !found.contains(path))
            .collect();
        if children.is_empty() {
            return found;
        }
        found.extend(children);
    }
}
