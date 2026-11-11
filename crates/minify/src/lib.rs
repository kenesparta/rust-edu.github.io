//! Makes what the site publishes smaller without changing what a browser reads in it.
//!
//! [`css`] reads a stylesheet as CSS's tokens, and for now takes away only what stands before the
//! first token and after the last one: white space and comments. No token is rewritten: the
//! result is the stylesheet with bytes taken out.
//!
//! The tokens are names, functions, at-keywords, strings in double quotes, numbers with their
//! `%` or unit, `#` with a name, and punctuation. `\` escapes, strings in single quotes, `url()`
//! without quotes, and characters outside ASCII that are not in a string or a comment, are errors
//! with a line and column; [`ErrorKind`] lists them.

mod token;

use std::fmt;

use crate::token::Kind;

/// A line and a column, both counted from 1; columns count characters, not bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

/// Where and why a stylesheet is not in the subset.
#[derive(Debug, PartialEq, Eq)]
pub struct Error {
    pub pos: Pos,
    pub kind: ErrorKind,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// A `/*` without its `*/`.
    OpenComment,
    /// A `"` without the `"` that ends the string on the same line.
    OpenString,
    /// A `\`, in a string or outside one: an escape, which can spell any character.
    Escape,
    /// A string in single quotes.
    SingleQuote,
    /// `url(` with an address that is not in quotes, which CSS reads by rules of its own.
    BareUrl,
    /// `<!--` or `-->`, which CSS skips between rules.
    HtmlComment,
    /// A character outside ASCII that is not in a string or a comment, such as a letter with an
    /// accent in a name, a no-break space or a byte order mark.
    NonAscii(char),
    /// An ASCII character that starts no token of the subset: a control character, `?`, `` ` ``,
    /// or a `#` or `@` with no name after it.
    Unexpected(char),
}

/// The stylesheet `text` as the site publishes it: without the white space and the comments
/// before its first token and after its last.
pub fn css(text: &str) -> Result<String, Error> {
    let tokens = token::scan(text).map_err(|(offset, kind)| Error {
        pos: pos(text, offset),
        kind,
    })?;
    let start = tokens.first().map_or(0, |token| token.start);
    let last = tokens.iter().rev().find(|token| token.kind != Kind::End);
    let end = last.map_or(start, |token| token.end);
    Ok(text.get(start..end).unwrap_or_default().to_owned())
}

/// The line and the column, in characters, of the byte `offset` of `text`.
fn pos(text: &str, offset: usize) -> Pos {
    let before = text.get(..offset).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let last = before.rsplit('\n').next().unwrap_or_default();
    let col = last.chars().count() + 1;
    Pos { line, col }
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
        let message = match self {
            Self::OpenComment => "this comment has no end; close it with `*/`",
            Self::OpenString => "this string has no closing `\"` on its line; close it there",
            Self::Escape => "`\\` escapes are not supported; write the character itself",
            Self::SingleQuote => "strings in single quotes are not supported; write `\"…\"`",
            Self::BareUrl => "the address of `url(` needs quotes; write `url(\"…\")`",
            Self::HtmlComment => "`<!--` and `-->` are HTML's comment marks; write `/* … */`",
            Self::NonAscii('\u{feff}') => "a byte order mark; save the stylesheet without one",
            Self::NonAscii(c) => {
                let code = u32::from(*c);
                return write!(
                    f,
                    "U+{code:04X} is outside ASCII and in no string or comment; write names in \
                     ASCII, and a space as a plain space"
                );
            }
            Self::Unexpected('#') => "this `#` starts no color or id; write a name after it",
            Self::Unexpected('@') => "this `@` starts no at-rule; write a name after it",
            Self::Unexpected(c) => {
                let c = c.escape_default();
                return write!(
                    f,
                    "`{c}` starts no token that the minifier knows; remove it"
                );
            }
        };
        f.write_str(message)
    }
}
