//! Smaller copies of the texts that the site publishes, made only by deleting: every byte of a
//! copy is a byte of its source, in the same order.
//!
//! [`css()`] writes a stylesheet without its comments, without the blanks at the start and the end
//! of each line, and without the lines that are then empty. A comment is no token of CSS, and
//! white space is one token however long it is, so the copy has the tokens of its source. The scan
//! knows only code, comments and strings: it reads no rule, selector or value, and rewrites no
//! name, number, color or quote. What would make it guess (a comment or a string that does not
//! close, a `\` outside a string, a `url(` without quotes, `@charset`, a byte order mark after the
//! start) is an error with a line and column; [`ErrorKind`] lists them.

mod css;

use std::fmt;

/// A line and a column, both counted from 1; columns count characters, not bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

/// Where and why the scan of a text stops.
#[derive(Debug, PartialEq, Eq)]
pub struct Error {
    pub pos: Pos,
    pub kind: ErrorKind,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// A `/*` in a stylesheet without its `*/`.
    UnclosedComment,
    /// A string in a stylesheet without its closing quote on the line where it starts; a `\` at
    /// the end of that line does not carry it on.
    UnclosedString,
    /// U+FEFF in the code of a stylesheet, anywhere but as its first character. It is a byte order
    /// mark only at the start; elsewhere a browser reads it as a character of a name, and a copy
    /// that lost what stood before it would start with it, as with a mark.
    ByteOrderMark,
    /// A `\` in a stylesheet outside a string. It starts an escape, which could hide a quote or a
    /// `/*` from the scan, and which may take the white space after it as its end, where the copy
    /// leaves blanks out.
    Backslash,
    /// A `url(` in a stylesheet with an address that is not in quotes, where a `/*` starts no
    /// comment and a quote no string.
    UnquotedUrl,
    /// `@charset` in a stylesheet, in any letter case. It names the file's encoding only as the
    /// very first bytes, so that the comment or the empty line removed before it would switch it
    /// on, and with another encoding than UTF-8 a browser would not read what the scan reads.
    Charset,
}

/// `text`, a stylesheet, as the site publishes it: each line without its comments and without the
/// blanks at its start and its end, and no line that is then empty. A line ends at a line feed,
/// and spaces, tabs, carriage returns and form feeds are blanks, so a stylesheet with CR LF line
/// ends is written with line feeds. The last line ends with a line feed only where it does in
/// `text`.
///
/// Strings are copied as they are. Of a comment that is all that parts two tokens, as in
/// `a/* … */b`, the empty comment `/**/` stays; a comment that starts with `/*!`, the mark of a
/// notice to keep, stays as written; and so does a byte order mark at the start of `text`.
pub fn css(text: &str) -> Result<String, Error> {
    css::minify(text)
}

impl Error {
    /// An error of `kind` at the byte `at` of `text`.
    pub(crate) fn at(text: &str, at: usize, kind: ErrorKind) -> Self {
        let before = text.get(..at).unwrap_or(text);
        let line = before.matches('\n').count() + 1;
        let last = before.rsplit('\n').next().unwrap_or_default();
        let col = last.chars().count() + 1;
        let pos = Pos { line, col };
        Self { pos, kind }
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.kind)
    }
}

impl std::error::Error for Error {}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnclosedComment => "this comment has no end; close it with `*/`",
            Self::UnclosedString => {
                "this string has no closing quote on its line; write it on one line"
            }
            Self::ByteOrderMark => {
                "U+FEFF is a byte order mark only at the start of a file, and part of a name \
                 here; delete it"
            }
            Self::Backslash => {
                "a `\\` outside a string is not supported; write the character itself, or put \
                 the text in quotes"
            }
            Self::UnquotedUrl => {
                "the address of this `url(` is not in quotes; write it as `url(\"/favicon.svg\")`"
            }
            Self::Charset => {
                "`@charset` is not supported; delete it, since the site serves a stylesheet as \
                 UTF-8"
            }
        })
    }
}
