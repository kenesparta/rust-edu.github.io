//! The stylesheet pass through its public API: what each stylesheet becomes, and where each error
//! points.

use edu_minify::ErrorKind::{
    BareUrl, Escape, HtmlComment, NonAscii, OpenComment, OpenString, SingleQuote, Unexpected,
};
use edu_minify::{ErrorKind, css};

/// What `stylesheet` becomes, or its error.
fn min(stylesheet: &str) -> String {
    css(stylesheet).unwrap_or_else(|error| format!("error {error}"))
}

/// Checks that each stylesheet of `cases` becomes the text beside it.
fn becomes(cases: &[(&str, &str)]) {
    for (stylesheet, expected) in cases {
        assert_eq!(min(stylesheet), *expected, "{stylesheet:?}");
    }
}

/// The line, column and kind of the error in `stylesheet`, or `None` if the pass takes it.
fn error(stylesheet: &str) -> Option<(usize, usize, ErrorKind)> {
    css(stylesheet)
        .err()
        .map(|error| (error.pos.line, error.pos.col, error.kind))
}

#[test]
fn a_stylesheet_loses_what_stands_around_its_tokens() {
    becomes(&[
        ("\n/* a */\n\na { b : c }\n/* d */\n", "a { b : c }"),
        ("a{}", "a{}"),
        ("/* only a comment */", ""),
        ("\n\n", ""),
    ]);
}

#[test]
fn no_token_is_rewritten() {
    let cases = [
        "a{content:\"/* no comment */\" \"a;b}c{\" \"  two  spaces  \"}",
        "a{background:url( \"a b.png\" ) no-repeat;color:#FBF8F3}",
        "a{width:1e3px;height:+.5px;margin:-.5em 0.50rem 1.5E+3}",
        "a{margin:1 e3 1e 3 1 -x 0 .5px 1 % 0 - 1}",
        "a#b.c[d|=\"e\"]::before{}",
    ];
    for stylesheet in cases {
        assert_eq!(min(stylesheet), stylesheet);
    }
}

#[test]
fn a_token_outside_the_subset_is_refused_where_it_starts() {
    let cases = [
        ("a{} /* open", 1, 5, OpenComment),
        ("a{content:\"x}", 1, 11, OpenString),
        ("a{content:\"x\ny\"}", 1, 11, OpenString),
        ("a{content:\"\\201C\"}", 1, 12, Escape),
        (".a\\:b{}", 1, 3, Escape),
        ("a{content:'x'}", 1, 11, SingleQuote),
        ("a{background:url(a.png)}", 1, 14, BareUrl),
        ("a{background:URL( a.png )}", 1, 14, BareUrl),
        ("a{background:url()}", 1, 14, BareUrl),
        ("<!-- a{} -->", 1, 1, HtmlComment),
        ("a{}\n-->", 2, 1, HtmlComment),
        ("\u{e9}{}", 1, 1, NonAscii('\u{e9}')),
        ("/* \u{e9} */ a\u{a0}b{}", 1, 10, NonAscii('\u{a0}')),
        ("\u{feff}a{}", 1, 1, NonAscii('\u{feff}')),
        ("a{color:#}", 1, 9, Unexpected('#')),
        ("a{color:@}", 1, 9, Unexpected('@')),
        ("a{width:1px ? 2px}", 1, 13, Unexpected('?')),
        ("a{}\0", 1, 4, Unexpected('\0')),
    ];
    for (stylesheet, line, col, kind) in cases {
        assert_eq!(error(stylesheet), Some((line, col, kind)), "{stylesheet:?}");
    }
}

#[test]
fn an_error_says_where_it_is_what_it_is_and_what_to_do() {
    let url = "error 2:5: the address of `url(` needs quotes; write `url(\"…\")`";
    assert_eq!(min("a { b: c }\nd{e:url(x)}"), url);
}
