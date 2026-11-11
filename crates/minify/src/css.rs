//! The scan of a stylesheet. At each character it is in code, in a comment or in a string: it
//! copies code and strings, leaves comments out, and ends each line of code without the blanks
//! around it. It stops at what could put a browser's tokenizer in another of the three than its
//! own, or give a browser other characters to read than the scan has.

use crate::{Error, ErrorKind};

/// The byte order mark, which some editors write at the start of a UTF-8 file.
const MARK: char = '\u{feff}';

/// The white space of CSS besides the line feed, which ends a line here: a space, a tab, a
/// carriage return and a form feed. A space outside ASCII, such as U+00A0, is no white space in
/// CSS but a character of a name.
const BLANKS: [char; 4] = [' ', '\t', '\r', '\u{c}'];

/// The rule that names a stylesheet's encoding.
const CHARSET: &str = "@charset";

/// What the scan is in.
#[derive(Clone, Copy)]
enum State {
    /// Code: everything outside comments and strings.
    Code,
    /// A comment, from the byte `start` of the stylesheet.
    Comment {
        start: usize,
        /// Whether code stands right before the comment, with no white space between them.
        tight: bool,
        /// Whether the comment starts with `/*!`, which asks to keep it.
        kept: bool,
    },
    /// A string in `quote`, from the byte `start` of the stylesheet.
    Quoted { start: usize, quote: char },
}

/// The stylesheet as written so far.
struct Written {
    text: String,
    /// The byte of `text` where its last line starts, the line that the scan is writing.
    line: usize,
}

/// The first error of a scan, at a byte offset of the stylesheet.
type Scan = Result<(), (usize, ErrorKind)>;

/// `css`, a stylesheet, as [`crate::css()`] writes it, or the first error of its scan.
pub(crate) fn minify(css: &str) -> Result<String, Error> {
    let mut written = Written {
        text: String::with_capacity(css.len()),
        line: 0,
    };
    // A browser takes the mark off before it reads the stylesheet, so the scan starts after it,
    // and so do the columns of the first line.
    let css = match css.strip_prefix(MARK) {
        Some(rest) => {
            written.text.push(MARK);
            written.line = written.text.len();
            rest
        }
        None => css,
    };
    match scan(css, &mut written) {
        Ok(()) => Ok(written.text),
        Err((at, kind)) => Err(Error::at(css, at, kind)),
    }
}

/// Reads `css` to its end, and adds its code and its strings to `written`.
fn scan(css: &str, written: &mut Written) -> Scan {
    let mut state = State::Code;
    let mut chars = css.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let next = chars.peek().map(|&(_, next)| next);
        state = match state {
            State::Code => {
                if let Some(error) = refused(css, at, c) {
                    return Err(error);
                }
                match c {
                    '/' if next == Some('*') => {
                        chars.next();
                        let is_code = |before: char| !BLANKS.contains(&before);
                        let tight = written.line().ends_with(is_code);
                        let kept = chars.peek().is_some_and(|&(_, mark)| mark == '!');
                        let start = at;
                        State::Comment { start, tight, kept }
                    }
                    '"' | '\'' => {
                        written.text.push(c);
                        let (start, quote) = (at, c);
                        State::Quoted { start, quote }
                    }
                    '\n' => {
                        written.end_line();
                        state
                    }
                    _ => {
                        written.code(c);
                        state
                    }
                }
            }
            State::Comment { start, tight, kept } if c == '*' && next == Some('/') => {
                chars.next();
                let end = at + 2;
                let rest = css.get(end..).unwrap_or_default();
                // White space or the end of the stylesheet parts the comment from what follows; a
                // comment that follows at once is left to say for both whether code does.
                let parted = rest.is_empty() || rest.starts_with(is_space);
                let glued = tight && !parted && !rest.starts_with("/*");
                if kept {
                    written
                        .text
                        .push_str(css.get(start..end).unwrap_or_default());
                } else if glued {
                    // An empty comment still parts the two tokens, and nothing else does so
                    // without a change: a space would make `.a/* … */.b` the selector `.a .b`,
                    // and nothing at all would make `0/* … */1px` the number `01px`.
                    written.text.push_str("/**/");
                }
                State::Code
            }
            State::Comment { .. } => state,
            State::Quoted { start, quote } => match c {
                // The character after a `\` is part of the string, a quote too.
                '\\' => match chars.next() {
                    Some((_, escaped)) if !ends_line(escaped) => {
                        written.text.push(c);
                        written.text.push(escaped);
                        state
                    }
                    // A `\` before a line end would carry the string on to the next line.
                    Some(_) | None => return Err((start, ErrorKind::UnclosedString)),
                },
                _ if ends_line(c) => return Err((start, ErrorKind::UnclosedString)),
                _ => {
                    written.text.push(c);
                    if c == quote { State::Code } else { state }
                }
            },
        };
    }
    match state {
        State::Code => {
            written.drop_blanks();
            Ok(())
        }
        State::Comment { start, .. } => Err((start, ErrorKind::UnclosedComment)),
        State::Quoted { start, .. } => Err((start, ErrorKind::UnclosedString)),
    }
}

/// The error of the character `c` of code at the byte `at` of `css`, and where it starts, if the
/// scan stops there.
fn refused(css: &str, at: usize, c: char) -> Option<(usize, ErrorKind)> {
    let rest = css.get(at..).unwrap_or_default();
    let is_charset = |name: &str| name.eq_ignore_ascii_case(CHARSET);
    match c {
        MARK => Some((at, ErrorKind::ByteOrderMark)),
        '\\' => Some((at, ErrorKind::Backslash)),
        '@' if rest.get(..CHARSET.len()).is_some_and(is_charset) => Some((at, ErrorKind::Charset)),
        '(' => unquoted_url(css, at).map(|start| (start, ErrorKind::UnquotedUrl)),
        _ => None,
    }
}

/// Where the name `url` starts, if the `(` at the byte `at` of `css` follows it, in any letter
/// case, and no quote follows the `(` and the white space after it. The address of such a `url(`
/// runs to its `)`, and a `/*` or a quote in it is part of the address.
fn unquoted_url(css: &str, at: usize) -> Option<usize> {
    let start = at.checked_sub(3)?;
    let name = css.get(start..at)?;
    let is_name = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii();
    let before = css.get(..start)?;
    // A longer name, such as `curl`, is a function like any other; the `--` of a `<!--` is no
    // part of a name.
    let whole = !before.ends_with(is_name) || before.ends_with("<!--");
    let after = css.get(at + 1..)?.trim_start_matches(is_space);
    let quoted = after.starts_with(['"', '\'']);
    (name.eq_ignore_ascii_case("url") && whole && !quoted).then_some(start)
}

/// Whether `c` is white space in CSS.
fn is_space(c: char) -> bool {
    c == '\n' || BLANKS.contains(&c)
}

/// Whether `c` ends a line for a string of CSS, which holds no line feed, carriage return or form
/// feed.
fn ends_line(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{c}')
}

impl Written {
    /// The line that the scan is writing.
    fn line(&self) -> &str {
        self.text.get(self.line..).unwrap_or_default()
    }

    /// Adds a character of code, unless it is a blank at the start of the line.
    fn code(&mut self, c: char) {
        if !self.line().is_empty() || !BLANKS.contains(&c) {
            self.text.push(c);
        }
    }

    /// Takes the blanks off the end of the line.
    fn drop_blanks(&mut self) {
        while self.line().ends_with(BLANKS) {
            self.text.pop();
        }
    }

    /// Ends the line at a line feed of the stylesheet: without the blanks at its end, and with
    /// the line feed unless nothing is left of the line.
    fn end_line(&mut self) {
        self.drop_blanks();
        if !self.line().is_empty() {
            self.text.push('\n');
        }
        self.line = self.text.len();
    }
}
