//! The stylesheet pass through its public API: what each stylesheet is written as, where each
//! error points, and what holds for every stylesheet that passes: its copy is the stylesheet with
//! bytes deleted, passes again as it is, and has its tokens.

use std::fs;
use std::path::Path;

use edu_minify::ErrorKind::{
    Backslash, ByteOrderMark, Charset, UnclosedComment, UnclosedString, UnquotedUrl,
};
use edu_minify::{ErrorKind, css};

/// The white space of CSS.
const SPACE: [char; 5] = [' ', '\t', '\n', '\r', '\u{c}'];

/// What `source` is written as, or its error.
fn written(source: &str) -> String {
    css(source).unwrap_or_else(|error| format!("error {error}"))
}

/// The line, column and kind of the error in `source`, or `None` if it passes.
fn error(source: &str) -> Option<(usize, usize, ErrorKind)> {
    css(source)
        .err()
        .map(|error| (error.pos.line, error.pos.col, error.kind))
}

/// Whether `copy` is `source` with bytes deleted: none added, changed or moved.
fn is_cut_from(copy: &str, source: &str) -> bool {
    let mut source = source.bytes();
    copy.bytes().all(|byte| source.any(|other| other == byte))
}

/// The string that starts `css`, in `quote`, and the text after it, or `None` if the string does
/// not end on its line.
fn string(css: &str, quote: char) -> Option<(&str, &str)> {
    let ends_line = |c: char| matches!(c, '\n' | '\r' | '\u{c}');
    let mut chars = css.char_indices().skip(1);
    while let Some((at, c)) = chars.next() {
        match c {
            // The character after a `\` is part of the string, but for a line end.
            '\\' => {
                let (_, escaped) = chars.next()?;
                if ends_line(escaped) {
                    return None;
                }
            }
            _ if ends_line(c) => return None,
            _ if c == quote => return Some(css.split_at(at + 1)),
            _ => {}
        }
    }
    None
}

/// The tokens of `css` as far as a copy has to keep them, by a reading of its own that shares no
/// code with the pass: each string, and each run of other code that white space, a comment or a
/// string ends, with whether white space stands between it and the one before. A comment is no
/// token of CSS and white space is one token however long, so two stylesheets with the same list
/// have the same tokens. `None` for a stylesheet that the pass has to refuse: one with a comment
/// or a string that does not close, or with U+FEFF, a `\`, a `url(` without quotes or `@charset`
/// in its code.
fn tokens(css: &str) -> Option<Vec<(bool, String)>> {
    let is_name = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii();
    let mut tokens: Vec<(bool, String)> = Vec::new();
    // Whether white space came since the last token, and whether that token is code that the
    // next character goes on.
    let (mut spaced, mut open) = (false, false);
    // The code of that token in lowercase, to know `URL(` and `@CHARSET` in it.
    let mut lowercase = String::new();
    let mut rest = css.strip_prefix('\u{feff}').unwrap_or(css);
    while let Some(c) = rest.chars().next() {
        let after = &rest[c.len_utf8()..];
        if let Some(comment) = rest.strip_prefix("/*") {
            (_, rest) = comment.split_once("*/")?;
            open = false;
        } else if SPACE.contains(&c) {
            (spaced, open, rest) = (!tokens.is_empty(), false, after);
        } else if c == '"' || c == '\'' {
            let (string, after) = string(rest, c)?;
            tokens.push((spaced, string.to_owned()));
            (spaced, open, rest) = (false, false, after);
        } else {
            if !open {
                tokens.push((spaced, String::new()));
                lowercase.clear();
            }
            tokens.last_mut()?.1.push(c);
            lowercase.push(c.to_ascii_lowercase());
            // `url` is a whole name after `<!--`, whose `--` is no part of it.
            let is_whole = |before: &str| !before.ends_with(is_name) || before.ends_with("<!--");
            let url = lowercase.strip_suffix("url(").is_some_and(is_whole);
            let quoted = after.trim_start_matches(SPACE).starts_with(['"', '\'']);
            let refused = c == '\u{feff}' || c == '\\' || lowercase.ends_with("@charset");
            if refused || (url && !quoted) {
                return None;
            }
            (spaced, open, rest) = (false, true, after);
        }
    }
    Some(tokens)
}

/// Asserts of `source` what holds for every text, and says whether it passes. If it does, its
/// copy is `source` with bytes deleted, passes again as it is, and has the tokens of `source`. If
/// it does not, `source` is one that the pass has to refuse, and the error has a hint and points
/// at what starts the trouble.
fn check(source: &str) -> bool {
    match css(source) {
        Ok(copy) => {
            assert!(is_cut_from(&copy, source), "{source:?} -> {copy:?}");
            assert_eq!(css(&copy).as_deref(), Ok(copy.as_str()), "{source:?}");
            let expected = tokens(source);
            assert!(expected.is_some(), "{source:?} -> {copy:?}");
            assert_eq!(tokens(&copy), expected, "{source:?} -> {copy:?}");
            true
        }
        Err(error) => {
            assert_eq!(tokens(source), None, "{source:?}: {error}");
            assert!(error.kind.to_string().contains("; "), "{error}");
            let text = source.strip_prefix('\u{feff}').unwrap_or(source);
            let line = text.split('\n').nth(error.pos.line - 1).unwrap_or_default();
            let found: String = line.chars().skip(error.pos.col - 1).collect();
            let found = found.to_ascii_lowercase();
            let starts = ["/*", "\"", "'", "\u{feff}", "\\", "url(", "@charset"];
            let is_start = starts.iter().any(|start| found.starts_with(start));
            assert!(is_start, "{source:?}: {error}");
            false
        }
    }
}

#[test]
fn comments_go_with_the_blanks_around_each_line_and_the_lines_left_empty() {
    let source = concat!(
        "/*\n * The colors.\n */\n\n",
        ":root {\n    --paper: #fff; /* light */\n\n",
        "    /* Two\n       lines. */\n\tcolor: red;  \n}\n",
    );
    assert_eq!(written(source), ":root {\n--paper: #fff;\ncolor: red;\n}\n");
    assert!(check(source));
    let cases = [
        ("", ""),
        (" \n\t\n", ""),
        ("/* Only a comment. */", ""),
        ("a { b: c }", "a { b: c }"),
        ("\n\n  \na {\n  b: c;\n}\n\n\n", "a {\nb: c;\n}\n"),
        // The blanks inside a line stay, and so does a line break between two tokens.
        ("  a  {  b :  c  }  ", "a  {  b :  c  }"),
        ("a,\n  b { c: 0\n 1px }\n", "a,\nb { c: 0\n1px }\n"),
        ("a { b: c; /* note */\n}", "a { b: c;\n}"),
        ("a {\n  /* note */ b: c\n}", "a {\nb: c\n}"),
        ("a {}\n/* one */\n\n/* two */\nb {}\n", "a {}\nb {}\n"),
        ("a {} /* one */ /* two */ b {}", "a {}   b {}"),
        // A comment over several lines ends none of them.
        ("a {} /* one\n two */ b {}\n", "a {}  b {}\n"),
        ("a { b: c } /* one\n two */\n", "a { b: c }\n"),
        ("/* * / ** // */a/* /* */", "a"),
    ];
    for (source, expected) in cases {
        assert_eq!(written(source), expected, "{source:?}");
        assert!(check(source), "{source:?}");
    }
}

#[test]
fn an_empty_comment_stays_where_a_comment_is_all_that_parts_two_tokens() {
    let cases = [
        (".a/* x */.b {}", ".a/**/.b {}"),
        ("a { margin: 0/* x */1px }", "a { margin: 0/**/1px }"),
        ("a/**/b", "a/**/b"),
        ("a/* x\n y */b", "a/**/b"),
        ("a//* x */*b", "a//**/*b"),
        ("url/* x */(\"a\")", "url/**/(\"a\")"),
        // The scan reads no token, so it does not know that `}` and `b` would not join.
        ("a {}/* x */b {}", "a {}/**/b {}"),
        // Comments in a row leave one, and it does not look back at a notice.
        ("a/* x *//* y *//* z */b", "a/**/b"),
        ("a/* x *//*! y */b", "a/*! y */b"),
        ("a/*! x *//* y */b", "a/*! x *//**/b"),
        // White space on one side, a line end or an end of the stylesheet parts them already.
        ("a /* x */b,c/* y */ d {}/* z */", "a b,c d {}"),
        ("a/* x */\nb", "a\nb"),
        ("a\n/* x */b", "a\nb"),
        ("a\t/* x */b\u{c}/* y */\rc", "a\tb\u{c}\rc"),
        ("/* x */a/* y */", "a"),
        ("a/* x *//* y */ b {}/**//* z */\n", "a b {}\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(written(source), expected, "{source:?}");
        assert!(check(source), "{source:?}");
    }
}

#[test]
fn a_comment_that_starts_with_an_exclamation_mark_stays_as_written() {
    let cases = [
        ("/*! A notice. */\n\na {}\n", "/*! A notice. */\na {}\n"),
        // Its own lines keep their blanks; the blanks around it go, as around code.
        ("  /*!\n * A  \n */  \n\na {}", "/*!\n * A  \n */\na {}"),
        ("a/*! x */b", "a/*! x */b"),
        ("a /*! x */ /* y */ b", "a /*! x */  b"),
        ("/*!*/", "/*!*/"),
        ("/* ! Not at the start. */a", "a"),
    ];
    for (source, expected) in cases {
        assert_eq!(written(source), expected, "{source:?}");
        assert!(check(source), "{source:?}");
    }
}

#[test]
fn a_string_is_copied_as_it_is_with_what_looks_like_a_comment_in_it() {
    let kept = [
        "a::before { content: \"/* no */  \\\" ' \\\\\"; }\nb { font-family: 'A  B', \"C\" }\n",
        "a { background: url( \"/a.svg\" ) }",
        "a { background: URL('a /* b */.svg') }",
        "a[b=\"c  /*\"] { content: \"\\201C\" '\\'' \"\" }",
        "a { b: \"\\ @charset \u{feff}\" }",
        "@import \"a.css\";",
    ];
    for source in kept {
        assert_eq!(written(source), source);
        assert!(check(source), "{source:?}");
    }
    assert_eq!(written("  \"a  \"  \n  ' b'  "), "\"a  \"\n' b'");
    // A longer name that ends with `url` is a function like any other, and `url(` in a string or
    // in a comment is no function.
    let cases = [
        ("a { b: curl(/* c */ d) }", "a { b: curl( d) }"),
        ("a { b: my-url(/* c */d) }", "a { b: my-url(/**/d) }"),
        ("a { b: éurl(d) }", "a { b: éurl(d) }"),
        ("\"url(c)\" /* url(d) \\ @charset \u{feff} */", "\"url(c)\""),
        (".charset { b: c }", ".charset { b: c }"),
        ("<!-- a { b: c } -->", "<!-- a { b: c } -->"),
    ];
    for (source, expected) in cases {
        assert_eq!(written(source), expected, "{source:?}");
        assert!(check(source), "{source:?}");
    }
}

#[test]
fn a_line_ends_at_a_line_feed_and_only_the_blanks_of_ascii_are_white_space() {
    let cases = [
        ("a {\r\n    b: c;\r\n}\r\n", "a {\nb: c;\n}\n"),
        ("\ta\t\n\u{c}b\u{c}\n\rc\r\n", "a\nb\nc\n"),
        // A carriage return alone ends no line, so it is a blank inside its line.
        ("a {\r    b: c;\r}\r", "a {\r    b: c;\r}"),
        // U+00A0 and U+2003 are characters of a name in CSS.
        ("\u{a0}a \u{2003}\n", "\u{a0}a \u{2003}\n"),
        ("a\u{a0}/* x */b", "a\u{a0}/**/b"),
    ];
    for (source, expected) in cases {
        assert_eq!(written(source), expected, "{source:?}");
        assert!(check(source), "{source:?}");
    }
}

#[test]
fn the_last_line_ends_with_a_line_feed_only_where_it_does_in_the_stylesheet() {
    let cases = [
        ("a {}", "a {}"),
        ("a {}  ", "a {}"),
        ("a {}\n", "a {}\n"),
        ("a {}\n\n \n", "a {}\n"),
        ("a {}\n/* end */", "a {}\n"),
        ("a {} /* end */\n", "a {}\n"),
        // The line feed inside the comment ends no line.
        ("a {}/* end\n */", "a {}"),
    ];
    for (source, expected) in cases {
        assert_eq!(written(source), expected, "{source:?}");
        assert!(check(source), "{source:?}");
    }
}

#[test]
fn a_byte_order_mark_stays_at_the_start_and_is_no_token() {
    let cases = [
        ("\u{feff}", "\u{feff}"),
        ("\u{feff}/* x */\n  a {}\n", "\u{feff}a {}\n"),
        ("\u{feff}/* x */a", "\u{feff}a"),
        ("\u{feff}  a", "\u{feff}a"),
    ];
    for (source, expected) in cases {
        assert_eq!(written(source), expected, "{source:?}");
        assert!(check(source), "{source:?}");
    }
    // The columns of the first line count from the character after the mark.
    assert_eq!(error("\u{feff}a \\"), Some((1, 3, Backslash)));
}

#[test]
fn what_would_make_the_scan_guess_is_an_error_where_it_starts() {
    let cases = [
        ("a {}\n  /* open", 2, 3, UnclosedComment),
        ("/*/", 1, 1, UnclosedComment),
        ("a {} /*! kept", 1, 6, UnclosedComment),
        ("a { content: \"x\n\" }", 1, 14, UnclosedString),
        ("a { content: 'x", 1, 14, UnclosedString),
        ("a { content: 'x\\\n' }", 1, 14, UnclosedString),
        ("a { content: \"x\\", 1, 14, UnclosedString),
        ("a { content: \"x\r\" }", 1, 14, UnclosedString),
        ("a { content: \"x\\\u{c}\" }", 1, 14, UnclosedString),
        // U+FEFF after the start would be a mark again in a copy that lost what stood before it.
        (" \u{feff}a {}", 1, 2, ByteOrderMark),
        ("/* x */\u{feff}a {}", 1, 8, ByteOrderMark),
        ("\u{feff}\u{feff}a {}", 1, 1, ByteOrderMark),
        ("a {}\n\u{feff}b {}", 2, 1, ByteOrderMark),
        // An apostrophe in code starts a string.
        ("a { b: \"it's\" }\nc { d: it's }", 2, 10, UnclosedString),
        (".a\\:b {}", 1, 3, Backslash),
        ("a { width: 1px\\9 }", 1, 15, Backslash),
        ("a {}\n\\", 2, 1, Backslash),
        ("a { background: URL(/a.svg) }", 1, 17, UnquotedUrl),
        ("a { background: url( \n a.svg ) }", 1, 17, UnquotedUrl),
        ("a { background: url() }", 1, 17, UnquotedUrl),
        ("a { b: url(/* c */\"d\") }", 1, 8, UnquotedUrl),
        ("@import url(a.css);", 1, 9, UnquotedUrl),
        ("a{b:url(", 1, 5, UnquotedUrl),
        // After `<!--`, which is a token of CSS, `url` is a name of its own.
        ("<!--url(a.svg) -->", 1, 5, UnquotedUrl),
        ("@charset \"utf-8\";\na {}", 1, 1, Charset),
        ("/* First. */\n @CHARSET \"latin1\";", 2, 2, Charset),
    ];
    for (source, line, col, kind) in cases {
        assert_eq!(error(source), Some((line, col, kind)), "{source:?}");
        assert!(!check(source), "{source:?}");
    }
}

#[test]
fn columns_count_characters_not_bytes() {
    assert_eq!(error("é { content: \"é"), Some((1, 14, UnclosedString)));
    assert_eq!(error("/* é */\n.é\\ {}"), Some((2, 3, Backslash)));
}

/// Every construct that the scan knows, and lines one character away from an error, for the random
/// test to damage.
const SAMPLE: &str = "\
/*! A notice to keep,
    on two lines. */
/* The colors. */
:root {
    color-scheme: light dark; /* both */
    --paper: #fbf8f3; --ink: #2a3439;
}

@media screen and (prefers-color-scheme: dark) {
\t:root:not([data-theme=\"light\"]) { --paper: #1b1815 }\r
}
a/**/b, a /* c */ b,a/* c */{ margin: 0/* 1px */2px; }
c/* d *//* e */f, g/**//* h */ i, j/* k *//*! l *//* m */n {}/* o *//* p */
q::before { content: \"\\201C /* no */ \\\" '\"; }
q::after { content: '\\201D \" \\\\'; }
.logo { background: url( \"/images/crab.svg\" ) no-repeat, URL('a b.svg') }
.café\u{a0}{ font-family: \"Noto Sans\", sans-serif }
p + p > img { width: calc(100% - 2 * var(--gap)); aspect-ratio: 16 / 9 }
<!-- .old { background: URL( 'c.svg' ) } -->
@supports ((-webkit-backdrop-filter: blur(1px)) or (backdrop-filter: blur(1px))) {
    .bar { backdrop-filter: blur(8px) }
}
/* The end. */
";

#[test]
fn a_damaged_stylesheet_keeps_its_tokens_or_is_an_error_never_a_panic() {
    assert!(check(SAMPLE));
    let sample: Vec<char> = SAMPLE.chars().collect();
    let damage = [
        '/', '*', '!', '"', '\'', '\\', '(', '@', ' ', '\t', '\r', '\u{c}', '\n', '\u{a0}',
        '\u{feff}', 'é',
    ];
    // xorshift64 with a fixed seed: every run tries the same 5,000 stylesheets, each with two
    // random characters replaced, inserted or removed.
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut passed = 0;
    for _ in 0..5_000 {
        let mut text = sample.clone();
        for _ in 0..2 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let [low, high, how, what, ..] = state.to_le_bytes();
            let at = usize::from(u16::from_le_bytes([low, high])) % text.len();
            let c = damage[usize::from(what) % damage.len()];
            match how % 3 {
                0 => text[at] = c,
                1 => text.insert(at, c),
                _ => _ = text.remove(at),
            }
        }
        let text: String = text.into_iter().collect();
        passed += usize::from(check(&text));
    }
    // Both happen often, so neither what holds for a copy nor what holds for an error goes
    // unchecked.
    assert!((1_000..=4_000).contains(&passed), "{passed}");
}

#[test]
fn the_stylesheet_of_this_site_passes_and_keeps_its_tokens() {
    // The site of this repository, two folders above this crate. The test reads its stylesheet as
    // it is now: a copy kept here would be an older one after the next change to it.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2);
    let source = fs::read_to_string(root.unwrap().join("static/style.css")).unwrap();
    assert!(check(&source), "{:?}", css(&source).err());
    assert!(written(&source).len() < source.len());
}
