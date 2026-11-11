//! The stylesheet pass through its public API: what each stylesheet is written as, and where each
//! error points.

use std::fs;
use std::path::Path;

use edu_minify::ErrorKind::{
    Backslash, ByteOrderMark, Charset, UnclosedComment, UnclosedString, UnquotedUrl,
};
use edu_minify::{ErrorKind, css};

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

#[test]
fn comments_go_with_the_blanks_around_each_line_and_the_lines_left_empty() {
    let source = concat!(
        "/*\n * The colors.\n */\n\n",
        ":root {\n    --paper: #fff; /* light */\n\n",
        "    /* Two\n       lines. */\n\tcolor: red;  \n}\n",
    );
    assert_eq!(written(source), ":root {\n--paper: #fff;\ncolor: red;\n}\n");
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
    }
}

#[test]
fn columns_count_characters_not_bytes() {
    assert_eq!(error("é { content: \"é"), Some((1, 14, UnclosedString)));
    assert_eq!(error("/* é */\n.é\\ {}"), Some((2, 3, Backslash)));
}

#[test]
fn the_stylesheet_of_this_site_passes() {
    // The site of this repository, two folders above this crate. The test reads its stylesheet as
    // it is now: a copy kept here would be an older one after the next change to it.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2);
    let source = fs::read_to_string(root.unwrap().join("static/style.css")).unwrap();
    let copy = css(&source).unwrap();
    assert!(copy.len() < source.len());
}
