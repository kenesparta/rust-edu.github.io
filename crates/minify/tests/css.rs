//! The stylesheet pass through its public API: what each stylesheet becomes, and where each error
//! points.

use edu_minify::ErrorKind::{
    BareUrl, CommentInside, Escape, HtmlComment, Nested, NoBlock, NonAscii, NotDeclaration,
    OpenComment, OpenString, SingleQuote, StraySemicolon, Unbalanced, Unclosed, Unexpected,
    UnknownAtRule,
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
fn white_space_goes_around_braces_semicolons_commas_and_the_colon_of_a_declaration() {
    becomes(&[
        ("a:hover { color : red ; }", "a:hover{color:red}"),
        (" a { } b { } ", "a{}b{}"),
        ("a\n,\nb\n{\n}", "a,b{}"),
        ("a\r\n{\r\ncolor:\tred\r\n}\r\n", "a{color:red}"),
        ("a{-webkit-b: c 1s , d 2s}", "a{-webkit-b:c 1s,d 2s}"),
        ("a{width:max( 1px , 2px )}", "a{width:max( 1px,2px )}"),
        (":is( a , b ) c{}", ":is( a,b ) c{}"),
        ("@media (a:1) , (b:2){a{}}", "@media (a:1),(b:2){a{}}"),
        (
            "@MEDIA screen {\n  a { b : c }\n}\n",
            "@MEDIA screen{a{b:c}}",
        ),
        (
            "@media a { @supports (b:c) { d { } } }",
            "@media a{@supports (b:c){d{}}}",
        ),
        ("body { color: #222; }", "body{color:#222}"),
        ("\n\n", ""),
    ]);
}

#[test]
fn one_white_space_stays_wherever_else_two_tokens_have_some() {
    becomes(&[
        // In a selector it may be the descendant combinator, and the pass does not read selectors.
        ("a   :hover  >  b + c ~ d{}", "a :hover > b + c ~ d{}"),
        (
            "li:nth-child( 2n + 1 ) [a = \"b\" i]{}",
            "li:nth-child( 2n + 1 ) [a = \"b\" i]{}",
        ),
        ("a \n.b\t\n.c{d:e\tf}", "a .b\n.c{d:e\tf}"),
        ("a:has(> b , + c){}", "a:has(> b, + c){}"),
        // After `and`, a `(` without it would make a function.
        (
            "@media  not all  and  ( a : 1px ) {}",
            "@media not all and ( a : 1px ){}",
        ),
        (
            "@supports  selector(a > b) {}",
            "@supports selector(a > b){}",
        ),
        // In a value two tokens could join, and `calc()` reads `+` and `-` only between spaces.
        (
            "a{margin: 0   -1px;top: -1px  -  2px}",
            "a{margin:0 -1px;top:-1px - 2px}",
        ),
        ("a{width:max(1px , - 2px)}", "a{width:max(1px, - 2px)}"),
        ("a{color: red  !important}", "a{color:red !important}"),
        (
            "a{b: auto 16 / 9;c: d(1.8)  e(20px)}",
            "a{b:auto 16 / 9;c:d(1.8) e(20px)}",
        ),
        // A missing `;` stays a missing `;`.
        ("a{color: red\n  top: 0;}", "a{color:red top: 0}"),
        // An argument of white space alone stays, beside a comma too.
        (
            "a{b:f( );c:g(1, ,2);d:var(--x, )}",
            "a{b:f( );c:g(1, ,2);d:var(--x, )}",
        ),
    ]);
}

#[test]
fn the_last_semicolon_of_a_block_and_empty_declarations_go() {
    becomes(&[
        ("a{b:c;d:e;}", "a{b:c;d:e}"),
        ("a{;;b:c;;;d:e;;}", "a{b:c;d:e}"),
        ("a{;}", "a{}"),
        ("a{color:}", "a{color:}"),
        ("a{b:c ; }d{e:f}", "a{b:c}d{e:f}"),
    ]);
}

#[test]
fn the_value_of_a_custom_property_keeps_the_white_space_inside_it() {
    becomes(&[
        ("a{--x:  a   b  ;}", "a{--x:a   b}"),
        ("a{--x:\n  a ,\n  b\n ;c: d , e}", "a{--x:a ,\n  b;c:d,e}"),
        ("a{--x : calc( 1px + 2px )}", "a{--x:calc( 1px + 2px )}"),
        (
            "a{--X: 50% - 2rem;b:var( --X )}",
            "a{--X:50% - 2rem;b:var( --X )}",
        ),
        // An empty value keeps its white space: `--x:;` was not valid before 2021.
        ("a{--x:   ;--y: }", "a{--x: ;--y: }"),
        ("a{--x:;}", "a{--x:}"),
    ]);
}

#[test]
fn a_comment_goes_only_from_between_two_rules_or_two_declarations() {
    becomes(&[
        ("/* only a comment */", ""),
        ("/* a */ a{} /* b */ /* c */ b{}/* d */", "a{}b{}"),
        ("a{/* b */c:d;/* e */f:g;/**/}", "a{c:d;f:g}"),
        ("@media screen{/* c */a{}/**/}", "@media screen{a{}}"),
        ("a{} /*/ not the end */ b{}", "a{}b{}"),
    ]);
    let inside = [
        ("a/**/b{}", 1, 2),
        ("a /* c */{}", 1, 3),
        ("@media /* c */ screen{a{}}", 1, 8),
        ("a{color/* c */:red}", 1, 8),
        ("a{color:red\n/* c */}", 2, 1),
        ("a{--x: /* c */ a}", 1, 8),
    ];
    for (stylesheet, line, col) in inside {
        let expected = Some((line, col, CommentInside));
        assert_eq!(error(stylesheet), expected, "{stylesheet:?}");
    }
}

#[test]
fn a_stylesheet_that_is_not_rules_of_declarations_is_refused() {
    let at_rule = |name: &str| UnknownAtRule(name.to_owned());
    let cases = [
        ("@import \"a.css\";", 1, 1, at_rule("@import")),
        ("a{}\n@font-face{}", 2, 1, at_rule("@font-face")),
        ("@media a{@keyframes b{}}", 1, 10, at_rule("@keyframes")),
        ("a{};b{}", 1, 4, StraySemicolon),
        ("a{}\n  b;c{}", 2, 3, NoBlock),
        ("@media screen;", 1, 1, NoBlock),
        ("@media screen{a}", 1, 15, NoBlock),
        ("a{}}", 1, 4, Unbalanced('}')),
        ("a{width:1px)}", 1, 12, Unbalanced(')')),
        ("a(b]{}", 1, 4, Unbalanced(']')),
        ("@media screen{a{}", 1, 14, Unclosed('}')),
        ("a{}\n b{c:d", 2, 3, Unclosed('}')),
        ("a{width:calc(1px;}", 1, 9, Unclosed(')')),
        ("a{width:calc((1px", 1, 14, Unclosed(')')),
        ("a[b{}", 1, 2, Unclosed(']')),
        ("@media (a{a{}}", 1, 8, Unclosed(')')),
        (":is(a{}b){}", 1, 2, Unclosed(')')),
        ("a{b:f(c;d:e)}", 1, 5, Unclosed(')')),
        ("a{b{}}", 1, 3, NotDeclaration),
        ("a{color red}", 1, 3, NotDeclaration),
        ("a{\n  &:hover{}}", 2, 3, NotDeclaration),
        ("a{*zoom:1}", 1, 3, NotDeclaration),
        ("a{@media screen{b:c}}", 1, 3, NotDeclaration),
        ("a{b:c;{}}", 1, 7, NotDeclaration),
        ("a{b:c{}}", 1, 6, Nested),
        ("a{color:red\nb{color:blue}", 2, 2, Nested),
        ("a{--x: {a:b}}", 1, 8, Nested),
    ];
    for (stylesheet, line, col, kind) in cases {
        assert_eq!(error(stylesheet), Some((line, col, kind)), "{stylesheet:?}");
    }
}

#[test]
fn an_error_says_where_it_is_what_it_is_and_what_to_do() {
    let url = "error 2:5: the address of `url(` needs quotes; write `url(\"…\")`";
    assert_eq!(min("a { b: c }\nd{e:url(x)}"), url);
    let bracket = "error 1:8: no `)` closes this before the next `;`, `{` or `}`; add it";
    assert_eq!(min("a { b: (c }"), bracket);
    let at_rule = "error 1:1: the minifier knows the at-rules `@media` and `@supports`, not \
                   `@page`; teach it what the block of `@page` holds before a stylesheet uses it";
    assert_eq!(min("@page { margin: 0 }"), at_rule);
}
