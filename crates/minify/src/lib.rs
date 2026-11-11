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
//!
//! [`Markup`] writes the markup of a layout, which comes in runs between the layout's template
//! tags, with each run of white space that holds a line break as one line break: in text between
//! tags, between the attributes of a tag, and in a script without a `type`, where a line break
//! stays wherever one is, so that the script has the tokens and the lines of its source. The
//! values of attributes stay as written, and so does the text of `pre`, `textarea`, `title`,
//! `style` and a script with a `type`. The scan knows a tag, a value in double quotes and the end
//! of an element whose text is no markup, and reads no JavaScript. What would make it guess is
//! an error here too: an HTML comment, a `<` that starts no tag, a value that is not in double
//! quotes, a tag or such an element that does not close, or whose start tag ends in `/>`, a
//! `<` in such an element inside `svg`, and in a script a backtick or a `\` at the end of a line.

mod css;
mod markup;

use std::fmt;

pub use markup::Markup;

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
    /// `<!--` in a layout: in text it starts a comment, which the page would publish, and in a
    /// script an HTML parser may read past the `</script>` after it.
    HtmlComment,
    /// A `<` in the text of a layout that starts no tag: neither a letter, `/` and a letter, nor
    /// `!DOCTYPE html>` in any letter case follows it. An HTML parser would guess what it is.
    LessThan,
    /// An attribute in a layout that is not written `name="value"`: a value in single quotes or
    /// in none, or a `=` that does not stand right between the name and the first quote.
    UnquotedValue,
    /// A tag in a layout without its `>` before the next `<` or before the end of the layout.
    UnclosedTag,
    /// A `pre`, or an element whose text is no markup, such as `script` or `title`, that is
    /// still open at the end of a layout.
    UnclosedElement,
    /// A start tag in a layout that ends with `/>`, of an element whose text is no markup. The
    /// `/` ends such an element in an SVG image, and does nothing in HTML, whose parser reads on
    /// to an end tag.
    SelfClosed,
    /// A `<` in a layout that is not the start of the end tag, in the text of a `script`, a
    /// `style`, a `title` or another such element inside `svg` or `math`. A browser reads that
    /// text as markup there, and not as HTML's parser reads the text of such an element.
    ForeignMarkup,
    /// A backtick in a script of a layout. A template literal may hold line breaks, where every
    /// blank is part of its text.
    Backtick,
    /// A `\` at the end of a line in a script of a layout, which carries a string on to the next
    /// line, with the blanks that start that line.
    BrokenString,
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
            Self::HtmlComment => {
                "`<!--` starts an HTML comment, which a page would publish, and in a script it \
                 moves the end of the script; write a comment as `{# … #}`"
            }
            Self::LessThan => {
                "this `<` starts no tag, end tag or `<!DOCTYPE html>`; write `&lt;` for the \
                 character"
            }
            Self::UnquotedValue => {
                "an attribute is written as `name=\"value\"`; put its value in double quotes, \
                 with no space around the `=`"
            }
            Self::UnclosedTag => {
                "no `>` closes this tag; close it and each of its values, or write `&lt;` for a \
                 `<` that starts no tag"
            }
            Self::UnclosedElement => {
                "this element is still open at the end of the layout; close it with its end tag"
            }
            Self::SelfClosed => {
                "`/>` does not end this element in HTML, where its text runs on to an end tag; \
                 write the end tag, as in `<title></title>`"
            }
            Self::ForeignMarkup => {
                "inside `svg` and `math` a browser reads this `<` as markup, in a script or a \
                 `style` too; write `&lt;` for the character, or move the element out"
            }
            Self::Backtick => {
                "a backtick in a script is not supported, since its text may run over lines; \
                 join strings with `+`"
            }
            Self::BrokenString => {
                "a `\\` at the end of a line carries a string of a script on to the next line; \
                 write the string on one line, or join two with `+`"
            }
        })
    }
}
