//! The tokens of a stylesheet, as CSS Syntax Level 3 splits it, for the CSS that the site's
//! stylesheet uses. White space and comments are not tokens here. The scan stops at what it would
//! have to guess at, and at what the stylesheet has no use for; [`ErrorKind`] lists both.

use crate::ErrorKind;

/// CSS's white space: a space, a tab, a line feed, a carriage return and a form feed. Unicode's
/// other spaces, such as U+00A0, are not white space in CSS.
const SPACE: [char; 5] = [' ', '\t', '\n', '\r', '\x0C'];

/// The first error of a scan, at a byte of the stylesheet.
pub(crate) type Fail = (usize, ErrorKind);

/// A token of a stylesheet.
pub(crate) struct Token {
    pub(crate) kind: Kind,
    /// The byte of the stylesheet where the token starts.
    pub(crate) start: usize,
    /// The byte after the token's last.
    pub(crate) end: usize,
}

/// What a token is, as far as a stylesheet's blocks tell tokens apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A token that selectors and values are made of.
    Part(Part),
    /// `;`
    Semicolon,
    /// `{`
    Open,
    /// `}`
    Close,
    /// The end of the stylesheet, after its last token.
    End,
}

/// What a token of a selector or a value is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// A name: `main`, `site-nav`, `-webkit-backdrop-filter`, `--color-bg`.
    Ident,
    /// `@` and a name: `@media`.
    AtKeyword,
    /// A name and the `(` right after it: `var(`.
    Function,
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// `+` or `-` on its own, not the sign of a number.
    Sign,
    /// `(` or `[`
    Open(Bracket),
    /// `)` or `]`
    Close(Bracket),
    /// A token that is copied and never asked about: `#` and a name, a string, a number with its
    /// `%` or unit, or one of the delimiters `.` `>` `~` `*` `/` `=` `$` `^` `|` `!` `<` `&` `%`.
    Other,
}

/// The shape of a bracket other than `{` and `}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Bracket {
    Round,
    Square,
}

/// Reads `text`, a stylesheet, to its end and gives its tokens; the last one is [`Kind::End`].
pub(crate) fn scan(text: &str) -> Result<Vec<Token>, Fail> {
    let mut scanner = Scanner { text, offset: 0 };
    let mut tokens = Vec::new();
    loop {
        scanner.gap()?;
        let start = scanner.offset;
        let kind = scanner.token()?;
        let end = scanner.offset;
        tokens.push(Token { kind, start, end });
        if kind == Kind::End {
            return Ok(tokens);
        }
    }
}

/// A scan of a stylesheet, at a byte offset that only ever moves forward.
struct Scanner<'a> {
    text: &'a str,
    offset: usize,
}

impl<'a> Scanner<'a> {
    /// The text from the offset on.
    fn rest(&self) -> &'a str {
        self.text.get(self.offset..).unwrap_or_default()
    }

    /// The byte `ahead` of the offset. Every character that the scan asks about is an ASCII one,
    /// which no byte of another character equals.
    fn byte(&self, ahead: usize) -> Option<u8> {
        self.text.as_bytes().get(self.offset + ahead).copied()
    }

    /// Moves past the white space and the comments at the offset.
    fn gap(&mut self) -> Result<(), Fail> {
        loop {
            let rest = self.rest();
            if rest.starts_with(SPACE) {
                self.offset += 1;
            } else if rest.starts_with("/*") {
                let Some(len) = rest.get(2..).and_then(|inside| inside.find("*/")) else {
                    return Err((self.offset, ErrorKind::OpenComment));
                };
                self.offset += len + 4;
            } else {
                return Ok(());
            }
        }
    }

    /// Moves past the token at the offset and gives its kind.
    fn token(&mut self) -> Result<Kind, Fail> {
        let start = self.offset;
        let rest = self.rest();
        let Some(c) = rest.chars().next() else {
            return Ok(Kind::End);
        };
        if c == '"' {
            return self.string();
        }
        if self.starts_number() {
            self.number();
            return Ok(Kind::Part(Part::Other));
        }
        // CSS reads the two marks as tokens of their own, and skips them between rules.
        if rest.starts_with("<!--") || rest.starts_with("-->") {
            return Err((start, ErrorKind::HtmlComment));
        }
        if self.starts_name(0) {
            return self.name_or_function();
        }
        self.offset += c.len_utf8();
        let part = match c {
            '@' if self.starts_name(0) => {
                self.name();
                Part::AtKeyword
            }
            '#' if self.byte(0).is_some_and(is_name) => {
                self.name();
                Part::Other
            }
            '{' => return Ok(Kind::Open),
            '}' => return Ok(Kind::Close),
            ';' => return Ok(Kind::Semicolon),
            ':' => Part::Colon,
            ',' => Part::Comma,
            '(' => Part::Open(Bracket::Round),
            ')' => Part::Close(Bracket::Round),
            '[' => Part::Open(Bracket::Square),
            ']' => Part::Close(Bracket::Square),
            '+' | '-' => Part::Sign,
            '.' | '>' | '~' | '*' | '/' | '=' | '$' | '^' | '|' | '!' | '<' | '&' | '%' => {
                Part::Other
            }
            '\'' => return Err((start, ErrorKind::SingleQuote)),
            '\\' => return Err((start, ErrorKind::Escape)),
            _ if c.is_ascii() => return Err((start, ErrorKind::Unexpected(c))),
            _ => return Err((start, ErrorKind::NonAscii(c))),
        };
        Ok(Kind::Part(part))
    }

    /// Moves past a string in double quotes, which ends on its line and has no `\` in it.
    fn string(&mut self) -> Result<Kind, Fail> {
        let start = self.offset;
        let inside = self.rest().get(1..).unwrap_or_default();
        for (at, c) in inside.char_indices() {
            match c {
                '"' => {
                    self.offset += at + 2;
                    return Ok(Kind::Part(Part::Other));
                }
                // An escape could hide the quote that ends the string.
                '\\' => return Err((start + 1 + at, ErrorKind::Escape)),
                '\n' | '\r' | '\x0C' => break,
                _ => {}
            }
        }
        Err((start, ErrorKind::OpenString))
    }

    /// Whether a number starts at the offset: a digit, or `.` and a digit, after `+`, `-` or
    /// nothing.
    fn starts_number(&self) -> bool {
        let sign = usize::from(matches!(self.byte(0), Some(b'+' | b'-')));
        let dot = usize::from(self.byte(sign) == Some(b'.'));
        self.byte(sign + dot).is_some_and(|c| c.is_ascii_digit())
    }

    /// Moves past a number: its sign, its digits with a `.` among them, its exponent, such as
    /// `e-3`, and its `%` or unit.
    fn number(&mut self) {
        self.offset += usize::from(matches!(self.byte(0), Some(b'+' | b'-')));
        self.digits();
        if self.byte(0) == Some(b'.') && self.byte(1).is_some_and(|c| c.is_ascii_digit()) {
            self.offset += 1;
            self.digits();
        }
        if matches!(self.byte(0), Some(b'e' | b'E')) {
            let sign = usize::from(matches!(self.byte(1), Some(b'+' | b'-')));
            if self.byte(1 + sign).is_some_and(|c| c.is_ascii_digit()) {
                self.offset += 1 + sign;
                self.digits();
            }
        }
        if self.byte(0) == Some(b'%') {
            self.offset += 1;
        } else if self.starts_name(0) {
            self.name();
        }
    }

    /// Moves past digits.
    fn digits(&mut self) {
        while self.byte(0).is_some_and(|c| c.is_ascii_digit()) {
            self.offset += 1;
        }
    }

    /// Whether a name starts `ahead` of the offset: a letter or `_`, after `-` or nothing, or
    /// `--`.
    fn starts_name(&self, ahead: usize) -> bool {
        let dash = usize::from(self.byte(ahead) == Some(b'-'));
        let starts = |c: u8| c.is_ascii_alphabetic() || c == b'_' || (dash == 1 && c == b'-');
        self.byte(ahead + dash).is_some_and(starts)
    }

    /// Moves past the characters of a name.
    fn name(&mut self) {
        while self.byte(0).is_some_and(is_name) {
            self.offset += 1;
        }
    }

    /// Moves past a name, and past the `(` right after it, which makes it a function.
    fn name_or_function(&mut self) -> Result<Kind, Fail> {
        let start = self.offset;
        self.name();
        if self.byte(0) != Some(b'(') {
            return Ok(Kind::Part(Part::Ident));
        }
        let name = self.text.get(start..self.offset).unwrap_or_default();
        self.offset += 1;
        // Without quotes, CSS reads all up to the `)` as one address, in which `/*` starts no
        // comment and a space is an error.
        let quoted = self.rest().trim_start_matches(SPACE);
        if name.eq_ignore_ascii_case("url") && !quoted.starts_with(['"', '\'']) {
            return Err((start, ErrorKind::BareUrl));
        }
        Ok(Kind::Part(Part::Function))
    }
}

/// Whether `c` can be in a name: an ASCII letter or digit, `_` or `-`.
fn is_name(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kind of each token of `text`, the end too.
    fn kinds(text: &str) -> Vec<Kind> {
        let tokens = scan(text).unwrap();
        tokens.iter().map(|token| token.kind).collect()
    }

    /// The text of each token of `text`, with `|` between two.
    fn texts(text: &str) -> String {
        let tokens = scan(text).unwrap();
        let texts: Vec<&str> = tokens.iter().map(|t| &text[t.start..t.end]).collect();
        texts.join("|")
    }

    #[test]
    fn each_kind_of_token_is_told_from_the_others() {
        let cases = [
            ("main", Part::Ident),
            ("--color-bg", Part::Ident),
            ("@media", Part::AtKeyword),
            ("var(", Part::Function),
            (":", Part::Colon),
            (",", Part::Comma),
            ("+", Part::Sign),
            ("-", Part::Sign),
            ("(", Part::Open(Bracket::Round)),
            ("]", Part::Close(Bracket::Square)),
            ("#fbf8f3", Part::Other),
            ("\"a ; b\"", Part::Other),
            ("-0.5e3rem", Part::Other),
            ("50%", Part::Other),
            (">", Part::Other),
        ];
        for (text, part) in cases {
            assert_eq!(kinds(text), [Kind::Part(part), Kind::End], "{text}");
        }
        let block = [Kind::Open, Kind::Semicolon, Kind::Close, Kind::End];
        assert_eq!(kinds(" {/* a */;\n} "), block);
    }

    #[test]
    fn a_token_ends_where_css_syntax_ends_it() {
        let cases = [
            ("a b/**/c", "a|b|c|"),
            ("-a --b a-1 +a -", "-a|--b|a-1|+|a|-|"),
            ("-1 - 2 -.5 +.5e-3x 1.5E+3", "-1|-|2|-.5|+.5e-3x|1.5E+3|"),
            (
                "1px 1e3 1e 1e-x 1.5% 1. 1-2 1--x",
                "1px|1e3|1e|1e-x|1.5%|1|.|1|-2|1--x|",
            ),
            (
                "a( b (c) url( \"d\") @e#f #1.5.5",
                "a(|b|(|c|)|url(|\"d\"|)|@e|#f|#1|.5|.5|",
            ),
            ("a:hover>b{c:d!e}", "a|:|hover|>|b|{|c|:|d|!|e|}|"),
        ];
        for (text, expected) in cases {
            assert_eq!(texts(text), expected, "{text}");
        }
    }
}
