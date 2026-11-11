//! Makes what the site publishes smaller without changing what a browser reads in it.
//!
//! [`css`] takes a stylesheet and gives it back without its comments, without the white space
//! around `{`, `}`, `;` and `,` and after the `:` of a declaration, and without the last `;` of a
//! block. Every other run of white space becomes one character of itself, and the value of a
//! custom property keeps the white space inside it. No token is rewritten: the result is the
//! stylesheet with bytes taken out, and its tokens, as CSS Syntax Level 3 splits them, are the
//! same ones.
//!
//! The stylesheet is rules of a selector and a block of declarations, at the top level or inside
//! `@media` and `@supports`. Comments stand between two rules or two declarations. Other at-rules,
//! rules inside rules, `\` escapes, strings in single quotes, `url()` without quotes, and
//! characters outside ASCII that are not in a string or a comment, are errors with a line and
//! column; [`ErrorKind`] lists them.

mod css;
mod token;

use std::fmt;

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
    /// An at-rule other than `@media` and `@supports`, by its name with the `@`.
    UnknownAtRule(String),
    /// A `;` where a rule can start.
    StraySemicolon,
    /// A selector, or an at-rule, that meets a `;`, a `}` or the end before its `{`.
    NoBlock,
    /// A `)`, `]` or `}`, which is given, that closes nothing, or not what was opened last.
    Unbalanced(char),
    /// A `(`, `[`, function or block without the `)`, `]` or `}` that closes it, which is given.
    Unclosed(char),
    /// Something other than `name: value` where a declaration can start, such as a rule inside
    /// a rule.
    NotDeclaration,
    /// A `{` in a value: a rule inside a rule whose selector has a `:`, or a `}` missing above.
    Nested,
    /// A comment inside a selector, the prelude of an at-rule or a declaration.
    CommentInside,
}

/// The stylesheet `text` as the site publishes it: without its comments, the white space that no
/// token needs and the last `;` of each block.
pub fn css(text: &str) -> Result<String, Error> {
    token::scan(text)
        .and_then(|tokens| css::minify(text, &tokens))
        .map_err(|(offset, kind)| Error {
            pos: pos(text, offset),
            kind,
        })
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
            Self::UnknownAtRule(name) => {
                return write!(
                    f,
                    "the minifier knows the at-rules `@media` and `@supports`, not `{name}`; \
                     teach it what the block of `{name}` holds before a stylesheet uses it"
                );
            }
            Self::StraySemicolon => "a `;` between two rules; remove it",
            Self::NoBlock => "this rule has no block before the next `;` or `}`; write `{ … }`",
            Self::Unbalanced(close) => {
                return write!(
                    f,
                    "this `{close}` closes nothing; remove it, or open it first"
                );
            }
            Self::Unclosed('}') => "no `}` closes this block; add it",
            Self::Unclosed(close) => {
                return write!(
                    f,
                    "no `{close}` closes this before the next `;`, `{{` or `}}`; add it"
                );
            }
            Self::NotDeclaration => {
                "this is not a declaration, `name: value;`; a rule cannot stand inside a rule, so \
                 a `}` may be missing above"
            }
            Self::Nested => {
                "a `{` in a value; write no rule inside a rule, or add the `;` or `}` missing above"
            }
            Self::CommentInside => {
                "a comment inside a selector or a declaration; put it between two rules or two \
                 declarations"
            }
        };
        f.write_str(message)
    }
}
