//! The scan of a layout's markup through its public API: what each layout's own text is written
//! as, and where each error points.

use edu_minify::ErrorKind::{HtmlComment, LessThan, UnclosedTag, UnquotedValue, UnreadElement};
use edu_minify::{Error, ErrorKind, Markup, Pos};

/// The pieces of `layout` in order: each comment of the template language, from `{#` to the first
/// `#}` after it, and each run of the layout's own text between two of them.
fn pieces(layout: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = layout;
    while !rest.is_empty() {
        let len = match rest.strip_prefix("{#") {
            Some(comment) => comment.find("#}").map_or(rest.len(), |at| at + 4),
            None => rest.find("{#").unwrap_or(rest.len()),
        };
        let (piece, after) = rest.split_at(len);
        pieces.push(piece);
        rest = after;
    }
    pieces
}

/// The position right after `text`, which starts at `pos`.
fn after(pos: Pos, text: &str) -> Pos {
    text.chars().fold(pos, |pos, c| {
        let (line, col) = match c {
            '\n' => (pos.line + 1, 1),
            _ => (pos.line, pos.col + 1),
        };
        Pos { line, col }
    })
}

/// `layout` as the scan writes it: its own text run by run, and each comment of the template as
/// it is.
fn minified(layout: &str) -> Result<String, Error> {
    let mut markup = Markup::default();
    let mut written = String::new();
    let mut pos = Pos { line: 1, col: 1 };
    for piece in pieces(layout) {
        if piece.starts_with("{#") {
            written.push_str(piece);
        } else {
            written.push_str(&markup.text(pos, piece)?);
        }
        pos = after(pos, piece);
    }
    markup.finish()?;
    Ok(written)
}

/// What `layout` is written as, or its error.
fn written(layout: &str) -> String {
    minified(layout).unwrap_or_else(|error| format!("error {error}"))
}

/// The line, column and kind of the error in `layout`, or `None` if it passes.
fn error(layout: &str) -> Option<(usize, usize, ErrorKind)> {
    minified(layout)
        .err()
        .map(|error| (error.pos.line, error.pos.col, error.kind))
}

#[test]
fn a_run_of_white_space_with_a_line_break_is_written_as_one_line_break() {
    let layout = concat!(
        "<main>\n\n    <h1>Title</h1>\n    <p>One  two\n       three.</p>   \n\n</main>\n\n",
        "<footer>\t<p>The  end</p> </footer>\n",
    );
    let expected = concat!(
        "<main>\n<h1>Title</h1>\n<p>One  two\nthree.</p>\n</main>\n",
        "<footer>\t<p>The  end</p> </footer>\n",
    );
    assert_eq!(written(layout), expected);
    let cases = [
        ("", ""),
        ("a", "a"),
        ("\n", "\n"),
        ("  \n\t\n  ", "\n"),
        ("\n\n<p>a</p>\n\n", "\n<p>a</p>\n"),
        // A run without a line break stays as it is, and so does a blank between two words.
        ("  a  b \t c  ", "  a  b \t c  "),
        (
            "<a>x</a>  <a>y</a>\t<a>z</a>",
            "<a>x</a>  <a>y</a>\t<a>z</a>",
        ),
        // No run goes: the two links stay apart, and so do the words around the line break.
        ("<a>x</a>\n\n    <a>y</a>", "<a>x</a>\n<a>y</a>"),
        ("<p>one\n    two</p>", "<p>one\ntwo</p>"),
        ("a \n b &amp; c > d \n", "a\nb &amp; c > d\n"),
        // A line ends at a line feed; a carriage return is a blank, alone on its line too.
        ("a\r\n  b\r\n\r\nc", "a\nb\nc"),
        ("a \r b\r\rc", "a \r b\r\rc"),
        // Only the blanks of ASCII are white space: U+00A0 shows, and so does a form feed.
        ("a\u{a0} \n \u{a0}b", "a\u{a0}\n\u{a0}b"),
        (
            "a \u{a0} b\u{2003}\n\u{2028}",
            "a \u{a0} b\u{2003}\n\u{2028}",
        ),
        ("a \u{c}\n b\n\u{c}\n", "a \u{c}\nb\n\u{c}\n"),
        ("<!DOCTYPE html>\n\n<html>\n", "<!DOCTYPE html>\n<html>\n"),
        ("<!doctype HTML>  <p>", "<!doctype HTML>  <p>"),
    ];
    for (layout, expected) in cases {
        assert_eq!(written(layout), expected, "{layout:?}");
    }
}

#[test]
fn white_space_between_the_parts_of_a_tag_is_written_alike_and_a_value_as_it_is() {
    let cases = [
        (
            "<a\n   href=\"/x/\"   class=\"b  c\"\n\n   hidden\n>t</a\n  >",
            "<a\nhref=\"/x/\"   class=\"b  c\"\nhidden\n>t</a\n>",
        ),
        (
            "<meta name=\"d\" content=\" a \n\n  b \">",
            "<meta name=\"d\" content=\" a \n\n  b \">",
        ),
        (
            "<p title=\"1 < 2 > 0, 'x', =y, <!-- z -->\">",
            "<p title=\"1 < 2 > 0, 'x', =y, <!-- z -->\">",
        ),
        ("<br/>\n<br />\n<br\n/>", "<br/>\n<br />\n<br\n/>"),
        (
            "<circle cx=\"1\"/>  <path d=\"M0 0\"\n  />",
            "<circle cx=\"1\"/>  <path d=\"M0 0\"\n/>",
        ),
        ("<input disabled \n checked>", "<input disabled\nchecked>"),
        ("<p\u{c}\n class=\"a\">", "<p\u{c}\nclass=\"a\">"),
        ("<P CLASS=\"a\"\n>x</P\n>", "<P CLASS=\"a\"\n>x</P\n>"),
        ("<my-el a=\"\"\n\nb=\"\">", "<my-el a=\"\"\nb=\"\">"),
        // What an HTML parser reads as part of a name is one for the scan too.
        ("<a=\"b\n\nc\">", "<a=\"b\nc\">"),
        ("<p a\"b \n c'd>", "<p a\"b\nc'd>"),
        (
            "</pre>\n\n<prefix>\n\n<titles>",
            "</pre>\n<prefix>\n<titles>",
        ),
    ];
    for (layout, expected) in cases {
        assert_eq!(written(layout), expected, "{layout:?}");
    }
}

#[test]
fn a_comment_of_the_template_parts_no_run_of_white_space() {
    let cases = [
        // Where a comment stood alone on its line, one line break is left.
        (
            "<ul>\n    {# The menu. #}\n    <li>",
            "<ul>\n{# The menu. #}<li>",
        ),
        ("a\n\n{# b #}\n{# c #}\n\nd", "a\n{# b #}{# c #}d"),
        ("<a\n  {# b #}\n  href=\"/\">", "<a\n{# b #}href=\"/\">"),
        // The blanks before a comment are written before the scan sees the line break after it.
        ("a  {# b #}  \n  c", "a  {# b #}\nc"),
        ("a  {# b #}  c", "a  {# b #}  c"),
    ];
    for (layout, expected) in cases {
        assert_eq!(written(layout), expected, "{layout:?}");
    }
}

#[test]
fn the_gaps_that_a_reader_sees_as_a_space_stay() {
    // The sentences of the footer stand apart by the line break between them.
    let footer =
        "<p>Content licensed <a href=\"/cc0/\">CC0</a>.\n            Source on GitHub.</p>";
    let expected = "<p>Content licensed <a href=\"/cc0/\">CC0</a>.\nSource on GitHub.</p>";
    assert_eq!(written(footer), expected);
    // A no-break space is no white space: it stays beside a blank, and beside a line break.
    let spaced = "<p>10\u{a0}km \u{a0} or\u{a0}\n    6\u{a0}miles</p>";
    assert_eq!(
        written(spaced),
        "<p>10\u{a0}km \u{a0} or\u{a0}\n6\u{a0}miles</p>"
    );
}

#[test]
fn what_would_make_the_scan_guess_is_an_error_where_it_starts() {
    let cases = [
        ("<p>a</p>\n  <!-- b -->", 2, 3, HtmlComment),
        ("<!---->", 1, 1, HtmlComment),
        ("a < b", 1, 3, LessThan),
        ("a <3", 1, 3, LessThan),
        ("a\n</>", 2, 1, LessThan),
        ("</ p>", 1, 1, LessThan),
        ("<?xml version=\"1.0\"?>", 1, 1, LessThan),
        ("<!DOCTYPE  html>", 1, 1, LessThan),
        ("<!DOCTYPE html SYSTEM \"a\">", 1, 1, LessThan),
        ("<![CDATA[a]]>", 1, 1, LessThan),
        ("<!-", 1, 1, LessThan),
        ("</!DOCTYPE html>", 1, 1, LessThan),
        ("a\n\n  <", 3, 3, LessThan),
        ("<a href='/'>", 1, 9, UnquotedValue),
        ("<a href=/ >", 1, 9, UnquotedValue),
        ("<a href=>", 1, 9, UnquotedValue),
        ("<a href= \"/\">", 1, 9, UnquotedValue),
        ("<a href =\"/\">", 1, 9, UnquotedValue),
        ("<a\n  =\"/\">", 2, 3, UnquotedValue),
        ("<a href=\"/\"=\"b\">", 1, 12, UnquotedValue),
        ("<a href={# b #}c>", 1, 16, UnquotedValue),
        ("<p a\u{c}=\"b\">", 1, 6, UnquotedValue),
        ("<p>a <b c</p>", 1, 6, UnclosedTag),
        ("<p\n  class=\"a\"\n", 1, 1, UnclosedTag),
        ("a\n <a href=\"/>b</a>", 2, 2, UnclosedTag),
        ("<p", 1, 1, UnclosedTag),
        ("<a href=", 1, 1, UnclosedTag),
        ("<p<b>", 1, 1, UnclosedTag),
        ("</p <b>", 1, 1, UnclosedTag),
        // For now: `pre`, and each element whose text is no markup.
        ("a\n<pre>b</pre>", 2, 1, UnreadElement),
        ("<p>\n  <SCRIPT src=\"/a.js\">", 2, 3, UnreadElement),
        ("<p{# a #}re>", 1, 1, UnreadElement),
        ("<textarea></textarea>", 1, 1, UnreadElement),
    ];
    for (layout, line, col, kind) in cases {
        assert_eq!(error(layout), Some((line, col, kind)), "{layout:?}");
    }
}

#[test]
fn an_error_is_where_the_layout_has_it_after_any_run_and_columns_count_characters() {
    let cases = [
        ("é <", 1, 3, LessThan),
        ("<p title=\"é\" é='é'>", 1, 16, UnquotedValue),
        ("é{# é\né #}é\n é{# é #}<é", 3, 10, LessThan),
    ];
    for (layout, line, col, kind) in cases {
        assert_eq!(error(layout), Some((line, col, kind)), "{layout:?}");
    }
}
