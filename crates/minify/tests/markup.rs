//! The scan of a layout's markup through its public API: what each layout's own text is written
//! as, where each error points, and what holds for every layout that passes: its copy is the
//! layout with bytes deleted, passes again as it is, and has its tags, its values and its text.

use std::fs;
use std::path::Path;

use edu_minify::ErrorKind::{
    Backtick, BrokenString, ForeignMarkup, HtmlComment, LessThan, SafeValue, SelfClosed, SplitPart,
    UnclosedElement, UnclosedTag, UnquotedValue, ValueInScript, ValueInTag,
};
use edu_minify::{Error, ErrorKind, Markup, Pos};

/// The marks that open and close a tag of the template language.
const MARKS: [(&str, &str); 3] = [("{{", "}}"), ("{%", "%}"), ("{#", "#}")];

/// The pieces of `layout` in order: each tag of the template language, from its opening mark to
/// the first closing mark after it, and each run of the layout's own text between two of them.
fn pieces(layout: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = layout;
    while !rest.is_empty() {
        let len = match MARKS.iter().find(|(open, _)| rest.starts_with(open)) {
            Some((_, close)) => rest[2..].find(close).map_or(rest.len(), |at| at + 4),
            None => {
                let next = MARKS.iter().filter_map(|(open, _)| rest.find(open)).min();
                next.unwrap_or(rest.len())
            }
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

/// `layout` as the scan writes it: its own text run by run, and each tag of the template as it
/// is. The scan hears of a `{{ … }}`, and of the start and the end of the part of an `if`, an
/// `else` or a `for`, at the first word of the tag, as `edu-site` tells it of a parsed layout.
fn minified(layout: &str) -> Result<String, Error> {
    let mut markup = Markup::default();
    let mut written = String::new();
    let mut pos = Pos { line: 1, col: 1 };
    for piece in pieces(layout) {
        if MARKS.iter().any(|(open, _)| piece.starts_with(open)) {
            let word = piece[2..].trim_start();
            let at = after(pos, &piece[..piece.len() - word.len()]);
            let is_safe = word.trim_end_matches(['}', ' ']).ends_with("safe");
            let first = word.split([' ', '%']).next().unwrap_or_default();
            match (&piece[..2], first) {
                ("{{", _) => markup.print(at, is_safe)?,
                ("{%", "if" | "for") => markup.start(),
                ("{%", "else") => {
                    markup.end(at)?;
                    markup.start();
                }
                ("{%", "endif" | "endfor") => markup.end(at)?,
                // A tag that the template language does not have, which only a damaged layout
                // has: a part with nothing in it.
                ("{%", _) => {
                    markup.start();
                    markup.end(at)?;
                }
                _ => {}
            }
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

/// Whether `copy` is `source` with bytes deleted: none added, changed or moved.
fn is_cut_from(copy: &str, source: &str) -> bool {
    let mut source = source.bytes();
    copy.bytes().all(|byte| source.any(|other| other == byte))
}

/// `text` with each run of spaces, tabs, line feeds and carriage returns that holds a line feed
/// as one line feed.
fn squeezed(text: &str) -> String {
    let is_blank = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    let mut squeezed = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (blanks, after) = rest.split_at(rest.len() - rest.trim_start_matches(is_blank).len());
        squeezed.push_str(if blanks.contains('\n') { "\n" } else { blanks });
        let (word, after) = after.split_at(after.find(is_blank).unwrap_or(after.len()));
        squeezed.push_str(word);
        rest = after;
    }
    squeezed
}

/// The elements whose text is no markup.
const KEPT: [&str; 11] = [
    "title",
    "textarea",
    "noscript",
    "style",
    "iframe",
    "xmp",
    "listing",
    "plaintext",
    "noembed",
    "noframes",
    "script",
];

/// What a page gets of `layout`, by a reading of its own that shares no code with the scan, for a
/// layout that the scan lets through: the layout's own text, squeezed, but for the values of
/// attributes, what is in `pre`, and the text of an element that is no markup other than a script
/// without a `type`, which are as written. A comment of the template is left out, since it writes
/// nothing, and another tag of the template is one character, since what it writes is not the
/// layout's. Two layouts with the same reading differ in nothing but white space that a line break
/// stands for.
fn reading(layout: &str) -> String {
    let own = pieces(layout)
        .into_iter()
        .map(|piece| match piece.get(..2) {
            Some("{#") => "",
            Some("{{" | "{%") => "\u{1}",
            _ => piece,
        });
    let own: String = own.collect();
    let is_space = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}');
    let ends_name = |c: char| is_space(c) || c == '/' || c == '>';
    let (mut read, mut pre, mut rest) = (String::new(), 0_usize, own.as_str());
    let shown = |text: &str, pre: usize| match pre {
        0 => squeezed(text),
        _ => text.to_owned(),
    };
    loop {
        let (text, tag) = rest.split_at(rest.find('<').unwrap_or(rest.len()));
        read.push_str(&shown(text, pre));
        if tag.is_empty() {
            return read;
        }
        // The name of the tag, and then names of attributes, each maybe with `="`, a value and
        // `"`, to the first `>` outside a value.
        let open = if tag.starts_with("</") { 2 } else { 1 };
        let named = tag[open..]
            .find(ends_name)
            .map_or(tag.len(), |at| open + at);
        let (name, mut attributes) = tag.split_at(named);
        read.push_str(name);
        let mut typed = false;
        while !attributes.is_empty() && !attributes.starts_with('>') {
            let names = attributes.find(['=', '>']).unwrap_or(attributes.len());
            let (names, after) = attributes.split_at(names);
            let is_type = |name: &str| name.eq_ignore_ascii_case("type");
            typed |= names.split(|c| is_space(c) || c == '/').any(is_type);
            read.push_str(&shown(names, pre));
            attributes = match after.strip_prefix("=\"") {
                Some(value) => {
                    let (value, after) = value.split_at(value.find('"').unwrap_or(value.len()));
                    read.push_str(&format!("=\"{value}"));
                    after
                }
                None => after,
            };
            if let Some(after) = attributes.strip_prefix(['=', '"']) {
                read.push('"');
                attributes = after;
            }
        }
        rest = attributes.strip_prefix('>').unwrap_or(attributes);
        read.push('>');
        let element = name.trim_start_matches(['<', '/']).to_ascii_lowercase();
        let is_end = name.starts_with("</");
        if element == "pre" {
            pre = if is_end {
                pre.saturating_sub(1)
            } else {
                pre + 1
            };
        } else if !is_end && KEPT.contains(&element.as_str()) {
            // The text of the element runs to `</`, its name and white space, `/` or `>`.
            let (lower, close) = (rest.to_ascii_lowercase(), format!("</{element}"));
            let mut ends = lower.match_indices(&close);
            let end = ends.find(|(at, _)| lower[at + close.len()..].starts_with(ends_name));
            let (text, after) = rest.split_at(end.map_or(rest.len(), |(at, _)| at));
            let is_javascript = element == "script" && !typed;
            read.push_str(&shown(text, if is_javascript { pre } else { 1 }));
            rest = after;
        }
    }
}

/// What each kind of error points at: the character there, or for a tag of the template the mark
/// before its first word.
const POINTS: [(ErrorKind, &str); 13] = [
    (HtmlComment, "<"),
    (LessThan, "<"),
    (ForeignMarkup, "<"),
    (UnclosedTag, "<"),
    (UnclosedElement, "<"),
    (SelfClosed, "<"),
    (UnquotedValue, ""),
    (ValueInTag, "{{"),
    (ValueInScript, "{{"),
    (SafeValue, "{{"),
    (SplitPart, "{%"),
    (Backtick, "`"),
    (BrokenString, "\\"),
];

/// Asserts of `layout` what holds for every layout, and says whether it passes. If it does, its
/// copy is `layout` with bytes deleted, passes again as it is, and has the reading of `layout`.
/// If it does not, the error has a hint and points at what starts the trouble.
fn check(layout: &str) -> bool {
    match minified(layout) {
        Ok(copy) => {
            assert!(is_cut_from(&copy, layout), "{layout:?} -> {copy:?}");
            assert_eq!(minified(&copy).as_deref(), Ok(copy.as_str()), "{layout:?}");
            assert_eq!(reading(&copy), reading(layout), "{layout:?} -> {copy:?}");
            true
        }
        Err(error) => {
            assert!(error.kind.to_string().contains("; "), "{error}");
            let line = layout
                .split('\n')
                .nth(error.pos.line - 1)
                .unwrap_or_default();
            let (before, found): (String, String) = {
                let mut chars = line.chars();
                let before = chars.by_ref().take(error.pos.col - 1).collect();
                (before, chars.collect())
            };
            let points = POINTS.iter().find(|(kind, _)| *kind == error.kind);
            let is_start = points.is_some_and(|(_, start)| match *start {
                "{{" | "{%" => before.trim_end().ends_with(start),
                start => found.starts_with(start),
            });
            assert!(is_start, "{layout:?}: {error}");
            false
        }
    }
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
    assert!(check(layout));
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
        assert!(check(layout), "{layout:?}");
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
        // A `/` ends no start tag unless the `>` comes right after it.
        ("<title/ >\n\n</title>", "<title/ >\n\n</title>"),
        (
            "<style / media=\"a/\">\n\n</style/>",
            "<style / media=\"a/\">\n\n</style/>",
        ),
        ("<pre/>\n\n</pre/>\n\n", "<pre/>\n\n</pre/>\n"),
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
        assert!(check(layout), "{layout:?}");
    }
}

#[test]
fn what_is_in_pre_and_the_text_of_an_element_that_is_no_markup_stay_as_written() {
    let kept = [
        "<pre>\n  a  \n\n\tb\n</pre>",
        "<pre class=\"a\"><code>\n\n  a\n</code>\n\n  <b\n  id=\"x\"\n>c</b\n >\n</pre>",
        "<pre><pre>\n\n</pre>\n\n</pre>",
        "<PRE>\n\n</Pre>",
        "<textarea name=\"t\">\n\n  a  \n\n</textarea>",
        "<title> A \n\n title </title>",
        "<style>\n\n  a { b: c }\n\n  /* d */\n</style>",
        "<iframe src=\"/x/\">\n\n</iframe>",
        "<noscript>\n\n  <p>Without \n\n scripts.</p>\n</noscript>",
        "<xmp>\n\n  <b>\n</xmp><listing>\n\n</listing><noembed>\n\n</noembed><noframes>\n\n</noframes>",
        // No tag starts in such a text, and it ends at `</`, its name and white space, `/` or `>`.
        "<title>a < b, <!-- c -->, <p\n\n>, </titles>, </ title>\n\n</TITLE\n>",
        "<textarea>\n\n</textarea/>",
        "<style>\n\n</stylesheet>\n\n</style\u{c}>",
        "<svg><title>News \n\n feeds</title></svg>",
    ];
    for layout in kept {
        assert_eq!(written(layout), layout);
        assert!(check(layout), "{layout:?}");
    }
    let cases = [
        (
            "<pre>\n\n</pre>\n\n<p>\n\n</p>",
            "<pre>\n\n</pre>\n<p>\n</p>",
        ),
        (
            "<p>\n\n<pre\n\n>\n\n</pre\n\n>\n\n",
            "<p>\n<pre\n>\n\n</pre\n\n>\n",
        ),
        (
            "<pre>\n\n</pre></pre>\n\n<pre>\n\n</pre>",
            "<pre>\n\n</pre></pre>\n<pre>\n\n</pre>",
        ),
        (
            "<title>\n\n</title \n\n id=\"a\"\n\n>\n\n",
            "<title>\n\n</title \nid=\"a\"\n>\n",
        ),
        (
            "<textarea>\n\n</textarea>\n\n<p>",
            "<textarea>\n\n</textarea>\n<p>",
        ),
        ("<title>1 <</title>\n\n<p>", "<title>1 <</title>\n<p>"),
        // In `svg` and `math` such a text is as written too, and a script loses its blanks.
        (
            "<svg>\n\n<style>\n\n.a { b: c }\n</style><script>\n\n  d();\n</script></SVG>",
            "<svg>\n<style>\n\n.a { b: c }\n</style><script>\nd();\n</script></SVG>",
        ),
        (
            "<svg/><math></math><style>a < b\n\n</style>",
            "<svg/><math></math><style>a < b\n\n</style>",
        ),
        // An end tag of such an element with no start tag before it is a tag like any other.
        ("</title>\n\n</script>\n\n", "</title>\n</script>\n"),
    ];
    for (layout, expected) in cases {
        assert_eq!(written(layout), expected, "{layout:?}");
        assert!(check(layout), "{layout:?}");
    }
}

#[test]
fn a_script_keeps_each_line_break_between_two_lines_and_loses_the_blanks_around_them() {
    let layout = concat!(
        "<script>\n    try {\n        var theme = get(\"the  theme\");   \n\n",
        "        if (a < b) {\r\n\t    b = /a  b/;\n        }\n    } catch (e) {}\n</script>\n",
    );
    let expected = concat!(
        "<script>\ntry {\nvar theme = get(\"the  theme\");\n",
        "if (a < b) {\nb = /a  b/;\n}\n} catch (e) {}\n</script>\n",
    );
    assert_eq!(written(layout), expected);
    assert!(check(layout));
    let cases = [
        ("<script>a\n\n  b</script>", "<script>a\nb</script>"),
        ("<script>  a  \n  </script>", "<script>  a\n</script>"),
        (
            "<script src=\"/a.js\">\n\n</script>",
            "<script src=\"/a.js\">\n</script>",
        ),
        // Where a statement ends may hang on a line break, so one stays wherever one is.
        (
            "<script>return\n    a\n    ++\n    b</script>",
            "<script>return\na\n++\nb</script>",
        ),
        (
            "<script>// a  b\n\n  /* c\n     d */\n</script>",
            "<script>// a  b\n/* c\nd */\n</script>",
        ),
        (
            "<script>\n  --> a\n  b-->c\n</script>",
            "<script>\n--> a\nb-->c\n</script>",
        ),
        // A `\` carries no line on unless a line feed ends the line.
        (
            "<script>a = \"\\\\\";\n  b = '\\n' \\ c</script>",
            "<script>a = \"\\\\\";\nb = '\\n' \\ c</script>",
        ),
        (
            "<script>a = \"b\\\r  c\";</script>",
            "<script>a = \"b\\\r  c\";</script>",
        ),
        // The script ends where an HTML parser ends it, in a string too.
        (
            "<script>a = \"</script>\n\n\";",
            "<script>a = \"</script>\n\";",
        ),
        (
            "<script>\n\n</scripts>\n\n</SCRIPT\n\n>\n\n",
            "<script>\n</scripts>\n</SCRIPT\n>\n",
        ),
        (
            "<script>a <b && c <!-d\n\n</script>",
            "<script>a <b && c <!-d\n</script>",
        ),
        (
            "<SCRIPT DEFER data-type=\"a\">\n\n</SCRIPT>",
            "<SCRIPT DEFER data-type=\"a\">\n</SCRIPT>",
        ),
    ];
    for (layout, expected) in cases {
        assert_eq!(written(layout), expected, "{layout:?}");
        assert!(check(layout), "{layout:?}");
    }
    // A script with a `type` may be no JavaScript, and stays as written.
    let kept = [
        "<script type=\"module\">\n\n  a = `b\n\n  c`;\n</script>",
        "<script type=\"application/ld+json\">\n\n  {\"a\":  \"b\\\n\"}\n</script>",
        "<script async TYPE>\n\n</script>",
        "<script\ntype=\"\">\n\n</script>",
    ];
    for layout in kept {
        assert_eq!(written(layout), layout);
        assert!(check(layout), "{layout:?}");
    }
}

#[test]
fn a_comment_of_the_template_parts_no_white_space_and_another_tag_of_it_does() {
    let cases = [
        // Where a comment stood alone on its line, one line break is left.
        (
            "<ul>\n    {# The menu. #}\n    <li>",
            "<ul>\n{# The menu. #}<li>",
        ),
        ("a\n\n{# b #}\n{# c #}\n\nd", "a\n{# b #}{# c #}d"),
        (
            "<script>\n  {# a #}\n  b();\n\n  {# c #}\n</script>",
            "<script>\n{# a #}b();\n{# c #}</script>",
        ),
        ("<a\n  {# b #}\n  href=\"/\">", "<a\n{# b #}href=\"/\">"),
        // The blanks before a comment are written before the scan sees the line break after it.
        ("a  {# b #}  \n  c", "a  {# b #}\nc"),
        ("a  {# b #}  c", "a  {# b #}  c"),
        // What a value is, and whether a part is there, is not for the scan to know.
        ("a \n {{ b }} \n c", "a\n{{ b }}\nc"),
        (
            "a  {{ b }}  \n\n  {{ c | safe }}\n",
            "a  {{ b }}\n{{ c | safe }}\n",
        ),
        (
            "<ul>\n    {% for a in b %}\n    <li>{{ a }}</li>\n    {% endfor %}\n</ul>",
            "<ul>\n{% for a in b %}\n<li>{{ a }}</li>\n{% endfor %}\n</ul>",
        ),
        (
            "{% if a %}\n\n<p>b</p>\n\n{% else %}\n\n{% endif %}\n\n",
            "{% if a %}\n<p>b</p>\n{% else %}\n{% endif %}\n",
        ),
        // A comment in the middle of a name or of an end tag is not there for a browser.
        (
            "<p{# a #}re>\n\n</pr{# b #}e>\n\n",
            "<p{# a #}re>\n\n</pr{# b #}e>\n",
        ),
        (
            "<title>\n\n</ti{# a #}tle>\n\n",
            "<title>\n\n</ti{# a #}tle>\n",
        ),
        (
            "<script ty{# a #}pe=\"b\">\n\n</script>",
            "<script ty{# a #}pe=\"b\">\n\n</script>",
        ),
    ];
    for (layout, expected) in cases {
        assert_eq!(written(layout), expected, "{layout:?}");
        assert!(check(layout), "{layout:?}");
    }
}

#[test]
fn a_value_goes_in_text_and_in_quotes_and_a_part_ends_where_it_starts() {
    let kept = [
        "{{ a }}<p class=\"b {{ c }}\" id=\"{{ d }}\">{{ e }}</p>{{ f | safe }}",
        "<title>{{ a }} - {{ b }}</title><textarea>{{ c }}</textarea>",
        "<noscript><img src=\"{{ a }}\"></noscript><pre>{{ b }}</pre>",
        "<title>{{ a }} < {{ b }}</title>",
        "{% for a in b %}<li class=\"c\">{{ a }}</li>{% endfor %}",
        "{% if a %}<div class=\"b\">{% endif %}c{% if a %}</div>{% endif %}",
        "{% if a %}<pre>{{ b }}</pre>{% else %}<script>c();</script>{% endif %}",
        "<title>{% if a %}{{ b }}{% else %}{{ c }} - {{ b }}{% endif %}</title>",
        "<a class=\"b{% if c %} d{% endif %}\" {% if e %}hidden {% endif %}href=\"/\">",
        "<a href=\"/\"{% if b %} aria-current=\"page\"{% endif %}>",
        "<script>{% if a %}b();{% endif %}</script>",
        "<pre>{% for a in b %}<b>{{ a }}</b>{% endfor %}</pre>",
        "{% for a in b %}{% if a %}{% else %}{% endif %}{% endfor %}",
    ];
    for layout in kept {
        assert_eq!(written(layout), layout);
        assert!(check(layout), "{layout:?}");
    }
}

#[test]
fn the_gaps_that_a_reader_sees_as_a_space_stay() {
    // The links of the home page's hero stand apart by what the lines of the `for` leave.
    let hero = concat!(
        "    <p>\n        {% for item in site.hero %}\n",
        "        <a href=\"{{ item.url }}\">{{ item.name }}</a>\n        {% endfor %}\n    </p>\n",
    );
    let expected = concat!(
        "    <p>\n{% for item in site.hero %}\n",
        "<a href=\"{{ item.url }}\">{{ item.name }}</a>\n{% endfor %}\n</p>\n",
    );
    assert_eq!(written(hero), expected);
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
    assert!(check(hero) && check(footer) && check(spaced));
}

#[test]
fn what_would_make_the_scan_guess_is_an_error_where_it_starts() {
    let cases = [
        ("<p>a</p>\n  <!-- b -->", 2, 3, HtmlComment),
        ("<!---->", 1, 1, HtmlComment),
        ("<script>\n  a = b<!--c;\n</script>", 2, 8, HtmlComment),
        ("<script type=\"a\"><!{# b #}--", 1, 18, HtmlComment),
        ("a < b", 1, 3, LessThan),
        ("a <3", 1, 3, LessThan),
        ("a\n</>", 2, 1, LessThan),
        ("</ p>", 1, 1, LessThan),
        ("<?xml version=\"1.0\"?>", 1, 1, LessThan),
        ("<!DOCTYPE  html>", 1, 1, LessThan),
        ("<!DOCTYPE html SYSTEM \"a\">", 1, 1, LessThan),
        ("<![CDATA[a]]>", 1, 1, LessThan),
        ("<!-", 1, 1, LessThan),
        ("<!x>{{ a }}", 1, 1, LessThan),
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
        // In `svg` and `math`, a browser reads a tag where HTML has the text of a script.
        ("<svg><script>a < b;</script>", 1, 16, ForeignMarkup),
        ("<MATH><title>a<b>", 1, 15, ForeignMarkup),
        ("<svg><svg></svg><style></styles>", 1, 24, ForeignMarkup),
        ("<svg><textarea><!{# a #}-", 1, 16, ForeignMarkup),
        ("<p>\n<svg><circle r=\"1\"/>", 2, 1, UnclosedElement),
        ("<svg>{{ a | safe }}</svg>", 1, 9, SafeValue),
        ("{% if a %}<svg>{% endif %}</svg>", 1, 19, SplitPart),
        ("<p>a <b c</p>", 1, 6, UnclosedTag),
        ("<p\n  class=\"a\"\n", 1, 1, UnclosedTag),
        ("a\n <a href=\"/>b</a>", 2, 2, UnclosedTag),
        ("<p", 1, 1, UnclosedTag),
        ("<a href=", 1, 1, UnclosedTag),
        ("<p<b>", 1, 1, UnclosedTag),
        ("</p <b>", 1, 1, UnclosedTag),
        ("a\n<pre>b", 2, 1, UnclosedElement),
        ("<pre><pre></pre>", 1, 1, UnclosedElement),
        ("<pre>a\n<pre>b", 1, 1, UnclosedElement),
        ("<p>\n  <script>a();</scripts>", 2, 3, UnclosedElement),
        ("<title>a</title><title>b</title", 1, 17, UnclosedElement),
        ("<textarea>", 1, 1, UnclosedElement),
        ("<style>a {}</ style>", 1, 1, UnclosedElement),
        ("<p>a</p><plaintext>b", 1, 9, UnclosedElement),
        // In an SVG image `<title/>` is a whole element, and in HTML the start of one.
        ("<svg><title/></svg>", 1, 6, SelfClosed),
        ("a\n<script src=\"/a.js\" />b();", 2, 1, SelfClosed),
        ("<textarea{# a #}/{# b #}>", 1, 1, SelfClosed),
        ("<p {{ a }}>", 1, 7, ValueInTag),
        ("<{{ a }}>", 1, 5, ValueInTag),
        ("<h{{ a }}>", 1, 6, ValueInTag),
        ("<a href={{ a }}>", 1, 12, ValueInTag),
        ("<a\n  href=\"/\" {{ a | safe }}>", 2, 15, ValueInTag),
        ("<title>a <{{ b }}</title>", 1, 14, ValueInTag),
        ("<script>a = \"{{ b }}\";</script>", 1, 17, ValueInScript),
        ("<script type=\"a/b\">{{ a | safe }}", 1, 23, ValueInScript),
        ("<style>a { b: {{ c }} }</style>", 1, 18, ValueInScript),
        ("<iframe>{{ a }}</iframe>", 1, 12, ValueInScript),
        ("<p class=\"{{ a | safe }}\">", 1, 14, SafeValue),
        ("<title>{{ a | safe }}</title>", 1, 11, SafeValue),
        ("<pre>\n{{ a|safe }}</pre>", 2, 4, SafeValue),
        ("{% if a %}<pre>{% endif %}</pre>", 1, 19, SplitPart),
        ("<p\n{% if a %}>{% else %}>{% endif %}", 2, 15, SplitPart),
        ("{% for a in b %}<a b=\"{% endfor %}", 1, 26, SplitPart),
        (
            "<a b=\"\"{% if a %}>{% else %}{% endif %}",
            1,
            22,
            SplitPart,
        ),
        ("<pre>{% if a %}</pre><pre>{% endif %}", 1, 30, SplitPart),
        ("<xmp>{% if a %}</xmp><xmp>{% endif %}", 1, 30, SplitPart),
        ("<script{% if a %} type=\"b\"{% endif %}>", 1, 30, SplitPart),
        (
            "<script {% if a %}type=\"b\" {% endif %}>",
            1,
            31,
            SplitPart,
        ),
        // A part that starts right after a name ends in another place, after what parts names.
        ("<li{% if a %} class=\"b\"{% endif %}>", 1, 27, SplitPart),
        ("<input {% if a %}checked{% endif %}>", 1, 28, SplitPart),
        ("<script>\n  a = `b`;\n</script>", 2, 7, Backtick),
        ("<script>// It`s</script>", 1, 14, Backtick),
        ("<script>a = \"b\\\n  c\";</script>", 1, 15, BrokenString),
        ("<script>a = 'b\\  \r\n';</script>", 1, 15, BrokenString),
        ("<script>// c:\\\n</script>", 1, 14, BrokenString),
        ("<script>a = \"b\\{# c #}\n", 1, 15, BrokenString),
    ];
    for (layout, line, col, kind) in cases {
        assert_eq!(error(layout), Some((line, col, kind)), "{layout:?}");
        assert!(!check(layout), "{layout:?}");
    }
}

#[test]
fn an_error_is_where_the_layout_has_it_after_any_run_and_columns_count_characters() {
    let cases = [
        ("é <", 1, 3, LessThan),
        ("<p title=\"é\" é='é'>", 1, 16, UnquotedValue),
        ("é{# é\né #}é\n é{# é #}<é", 3, 10, LessThan),
        ("{{ é }}é{# é\né #}é\n é{{ é }}<é", 3, 10, LessThan),
        ("<script>\n  é = 1; {# é #} `", 2, 18, Backtick),
        ("é{% if é %}é\n<p{% endif %}", 2, 6, SplitPart),
        ("é\r\n{{ é }}<p é {{ é }}>", 2, 16, ValueInTag),
    ];
    for (layout, line, col, kind) in cases {
        assert_eq!(error(layout), Some((line, col, kind)), "{layout:?}");
    }
}

/// Every construct that the scan knows, for the random test to damage.
const SAMPLE: &str = "\
<!DOCTYPE html>
<html lang=\"en\">
<head>
    <title>{% if page.home %}{{ site.title }}{% else %}{{ page.title }} - {{ site.title }}{% endif %}</title>
    <meta name=\"description\" content=\"{{ site.description }}\">
    <script>
        {# The reader's choice, before the page paints. #}
        try {
            var theme = localStorage.getItem(\"theme\");  \r
            if (theme < \"light\") { theme = '\\n'; }
        } catch (e) {}
    </script>
    <script type=\"module\">let a = `1
        2`;</script>
    <style>
        a::before { content: \"<  >\" }
    </style>
</head>
<body>
    {# The menu. #}
    <ul class=\"menu  main\">
        {% for item in site.menu %}
        <li><a href=\"{{ item.url }}\"{% if item.here %} aria-current=\"page\"{% endif %}>{{ item.name }}</a></li>
        {% endfor %}
    </ul>
    <p>Content licensed <a href=\"/cc0/\">CC0</a>.
        Source on\u{a0}<a href=\"/git/\">GitHub</a>.</p>
    <pre class=\"a\">  two  spaces
    <b>kept</b>

</pre>
    <textarea name=\"t\">
  a
</textarea>
    <noscript><img src=\"{{ site.pixel }}\"
        alt=\"\"></noscript>
    <svg width=\"18\" viewBox=\"0 0 16 16\"><title>Feed</title><circle cx=\"3\" r=\"1.5\"/></svg>
    <input type=\"checkbox\" checked
        disabled>
    {{ content | safe }}
</body>
</html>
";

#[test]
fn a_damaged_layout_keeps_its_markup_or_is_an_error_never_a_panic() {
    assert!(check(SAMPLE));
    let sample: Vec<char> = SAMPLE.chars().collect();
    let damage = [
        '<', '>', '"', '\'', '=', '/', '!', '-', '`', '\\', ' ', '\t', '\r', '\n', '\u{c}',
        '\u{a0}', '{', '}', '%', '#', 'é', 'e',
    ];
    // xorshift64 with a fixed seed: every run tries the same 5,000 layouts, each with two random
    // characters replaced, inserted or removed.
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
fn the_layouts_of_this_site_pass_and_keep_their_markup() {
    // The site of this repository, two folders above this crate. The test reads its layouts as
    // they are now: copies kept here would be older ones after the next change to them.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2);
    let mut layouts = 0;
    for entry in fs::read_dir(root.unwrap().join("layouts")).unwrap() {
        let layout = fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(check(&layout), "{:?}", minified(&layout).err());
        layouts += 1;
    }
    assert!(layouts > 0);
}
