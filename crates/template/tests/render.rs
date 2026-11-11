//! The template language through its public API: what each template renders with a context,
//! where each error points, and what a walk through a template meets.

use edu_template::ErrorKind::{
    BadPath, BadTag, NotList, NotText, StrayElse, StrayEndfor, StrayEndif, TooDeep, UnclosedTag,
    UnendedFor, UnendedIf, UnknownFilter, UnknownVariable,
};
use edu_template::{ErrorKind, Pos, Step, Template, Value};

fn text(text: &str) -> Value {
    Value::Str(text.to_owned())
}

fn map(entries: &[(&str, Value)]) -> Value {
    let entries = entries
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()));
    Value::Map(entries.collect())
}

/// The context of the tests: a site with two pages, and a page of its own.
fn context() -> Value {
    let pages = Value::List(vec![
        map(&[("title", text("One")), ("url", text("/one/"))]),
        map(&[("title", text("Two & <three>")), ("url", text("/two/"))]),
    ]);
    let none = Value::List(Vec::new());
    let site = map(&[
        ("title", text("Rust Edu")),
        ("pages", pages),
        ("none", none),
    ]);
    let page = map(&[
        ("title", text("A \"quoted\" 'title'")),
        ("content", text("<p>Text</p>")),
        ("draft", Value::Bool(false)),
        ("listed", Value::Bool(true)),
        ("note", text("1 < 2 & 3 > 2")),
    ]);
    map(&[("site", site), ("page", page), ("empty", text(""))])
}

/// What `template` renders with the test context, or its error.
fn render(template: &str) -> String {
    let rendered = Template::parse(template).and_then(|template| template.render(&context()));
    rendered.unwrap_or_else(|error| format!("error {error}"))
}

/// The line, column and kind of the error in `template`, or `None` if it renders.
fn error(template: &str) -> Option<(usize, usize, ErrorKind)> {
    let rendered = Template::parse(template).and_then(|template| template.render(&context()));
    rendered
        .err()
        .map(|error| (error.pos.line, error.pos.col, error.kind))
}

#[test]
fn a_value_is_escaped_unless_it_is_safe() {
    let template = "<h1 title=\"{{ page.title }}\">{{page.title}}</h1>\n{{ page.content | safe }}";
    let title = "A &quot;quoted&quot; &#39;title&#39;";
    let expected = format!("<h1 title=\"{title}\">{title}</h1>\n<p>Text</p>");
    assert_eq!(render(template), expected);
    let template = "{{ site.title }}: {{ page.content|safe }}{{ empty }}. {{ page.note }}";
    let expected = "Rust Edu: <p>Text</p>. 1 &lt; 2 &amp; 3 &gt; 2";
    assert_eq!(render(template), expected);
}

#[test]
fn other_text_is_kept_and_comments_are_left_out() {
    assert_eq!(
        render("a {b} { %c %} }} %}{# {{ note }} #}d"),
        "a {b} { %c %} }} %}d"
    );
}

#[test]
fn for_repeats_its_part_for_each_item_of_a_list_and_nests() {
    let template = concat!(
        "{% for page in site.pages %}<a href=\"{{ page.url }}\">{{ page.title }}</a>",
        "{% for other in site.pages %}.{% endfor %}\n{% endfor %}",
        "{% for x in site.none %}none{% endfor %}",
    );
    // In the loop, `page` is the loop's item, not the context's page.
    let expected = "<a href=\"/one/\">One</a>..\n<a href=\"/two/\">Two &amp; &lt;three&gt;</a>..\n";
    assert_eq!(render(template), expected);
    // The innermost loop wins, and after its loop a name is the context's again.
    let template = concat!(
        "{% for p in site.pages %}{% for p in site.pages %}{{ p.url }}{% endfor %}{% endfor %}",
        "{% for page in site.pages %}{% endfor %}{{ page.note }}",
    );
    let expected = "/one//two//one//two/1 &lt; 2 &amp; 3 &gt; 2";
    assert_eq!(render(template), expected);
}

#[test]
fn if_keeps_its_first_part_for_true_or_a_value_that_is_not_empty() {
    let cases = [
        ("site.title", "yes"),
        ("empty", "no"),
        ("page.listed", "yes"),
        ("page.draft", "no"),
        ("site.pages", "yes"),
        ("site.none", "no"),
        ("site", "yes"),
        ("nothing.here", "no"),
    ];
    for (path, expected) in cases {
        let template = format!("{{% if {path} %}}yes{{% else %}}no{{% endif %}}");
        assert_eq!(render(&template), expected, "{path}");
    }
    assert_eq!(render("a{% if empty %}b{% endif %}c"), "ac");
    let template =
        "{% for page in site.pages %}{% if page.url %}{{ page.title }} {% endif %}{% endfor %}";
    assert_eq!(render(template), "One Two &amp; &lt;three&gt; ");
}

#[test]
fn a_tag_outside_the_language_is_an_error_at_its_first_word() {
    let cases = [
        ("a {{ page.title", 1, 3, UnclosedTag("}}")),
        ("{% if page.draft\n", 1, 1, UnclosedTag("%}")),
        ("{# note", 1, 1, UnclosedTag("#}")),
        (
            "{% extends \"base.html\" %}",
            1,
            4,
            BadTag("extends \"base.html\"".into()),
        ),
        ("a\n{%  if a and b %}", 2, 5, BadTag("if a and b".into())),
        (
            "{{ page.authors | join }}",
            1,
            4,
            UnknownFilter("join".into()),
        ),
        ("\n  {{ page-title }}", 2, 6, BadPath("page-title".into())),
        ("{{ }}", 1, 4, BadPath(String::new())),
        ("{{ page..title }}", 1, 4, BadPath("page..title".into())),
        ("{{ 1st }}", 1, 4, BadPath("1st".into())),
        ("{% for 1st in site.pages %}", 1, 8, BadPath("1st".into())),
        ("{% for  a.b in site.pages %}", 1, 9, BadPath("a.b".into())),
        ("{% if a- %}", 1, 7, BadPath("a-".into())),
    ];
    for (template, line, col, kind) in cases {
        assert_eq!(error(template), Some((line, col, kind)), "{template:?}");
    }
}

#[test]
fn an_if_or_for_needs_its_end_in_order_and_nests_at_most_8_deep() {
    let cases = [
        ("{% endif %}", 1, 4, StrayEndif),
        ("{% endfor %}", 1, 4, StrayEndfor),
        ("{% else %}", 1, 4, StrayElse),
        (
            "{% if a %}{% else %}{% else %}{% endif %}",
            1,
            24,
            StrayElse,
        ),
        ("{% for a in b %}{% else %}{% endfor %}", 1, 20, StrayElse),
        ("{% for a in b %}{% endif %}", 1, 20, StrayEndif),
        ("{% if a %}{% endfor %}", 1, 14, StrayEndfor),
        ("{% if a %}\n{% for a in b %}", 2, 4, UnendedFor),
        ("{% if a %}", 1, 4, UnendedIf),
    ];
    for (template, line, col, kind) in cases {
        assert_eq!(error(template), Some((line, col, kind)), "{template:?}");
    }
    let eight = format!("{}x{}", "{% if a %}".repeat(8), "{% endif %}".repeat(8));
    assert_eq!(render(&eight), "");
    let nine = "{% if a %}".repeat(9);
    assert_eq!(error(&nine), Some((1, 84, TooDeep)));
}

#[test]
fn a_value_that_is_missing_or_of_the_wrong_kind_is_an_error_at_its_path() {
    let cases = [
        (
            "<p>{{ page.titel }}</p>",
            1,
            7,
            UnknownVariable("page.titel".into()),
        ),
        (
            "é {{ page.titel }}",
            1,
            6,
            UnknownVariable("page.titel".into()),
        ),
        (
            "{{ site.title.text }}",
            1,
            4,
            UnknownVariable("site.title.text".into()),
        ),
        ("a\n {{ site.pages }}", 2, 5, NotText("site.pages".into())),
        ("{{ page.draft }}", 1, 4, NotText("page.draft".into())),
        ("{{ site | safe }}", 1, 4, NotText("site".into())),
        (
            "{% for x in site.title %}{% endfor %}",
            1,
            13,
            NotList("site.title".into()),
        ),
        (
            "{% for x in nothing %}{% endfor %}",
            1,
            13,
            UnknownVariable("nothing".into()),
        ),
        (
            "{% for page in site.pages %}\n{{ page.url }} {{ page.date }}{% endfor %}",
            2,
            19,
            UnknownVariable("page.date".into()),
        ),
    ];
    for (template, line, col, kind) in cases {
        assert_eq!(error(template), Some((line, col, kind)), "{template:?}");
    }
}

/// Every construct of the language, for the random test to damage.
const SAMPLE: &str = "\
<title>{{ page.title }} - {{ site.title }}</title>{# a comment #}
{% if page.draft %}<p>Draft</p>{% else %}{{ page.content | safe }}{% endif %}
<ul>{% for page in site.pages %}
<li><a href=\"{{ page.url }}\">{{ page.title }}</a></li>{% endfor %}</ul>
";

/// A list with an item for each line, its text between comments and white space, and then an `if`
/// with both parts and one without an `else`.
const WALKED: &str = concat!(
    "<ul>\n  {% for item in items %}\n  <li>{{ item }}</li>{# no #}  \n{% endfor %}</ul>",
    "{% if a %}é{{ b | safe }}{% else %}{# c #}{% endif %}{%  if a %}{% endif %}",
);

#[test]
fn a_walk_meets_the_text_the_values_and_the_parts_in_the_order_of_the_source() {
    let mut template = Template::parse(WALKED).unwrap();
    let mut met = Vec::new();
    let mut note = |pos: Pos, step: Step<'_>| {
        let step = match step {
            Step::Text(text) => format!("{text:?}"),
            Step::Print { safe: false } => "a value".to_owned(),
            Step::Print { safe: true } => "a safe value".to_owned(),
            Step::Start => "start".to_owned(),
            Step::End => "end".to_owned(),
        };
        met.push((pos.line, pos.col, step));
        Ok::<_, ()>(())
    };
    template.walk(&mut note).unwrap();
    // A run of text is where it starts and a tag where its first word is; a comment only parts
    // two runs, and a part without text has its start and its end all the same.
    let expected = [
        (1, 1, "\"<ul>\\n  \""),
        (2, 6, "start"),
        (2, 26, "\"\\n  <li>\""),
        (3, 10, "a value"),
        (3, 17, "\"</li>\""),
        (3, 30, "\"  \\n\""),
        (4, 4, "end"),
        (4, 13, "\"</ul>\""),
        (4, 21, "start"),
        (4, 28, "\"é\""),
        (4, 32, "a safe value"),
        (4, 46, "end"),
        (4, 46, "start"),
        (4, 63, "end"),
        (4, 75, "start"),
        (4, 85, "end"),
    ];
    let expected = expected.map(|(line, col, step)| (line, col, step.to_owned()));
    assert_eq!(met, expected);
}

#[test]
fn a_walk_can_change_each_run_of_text_and_ends_at_the_first_error() {
    let mut template = Template::parse(WALKED).unwrap();
    let mut trim = |_: Pos, step: Step<'_>| {
        match step {
            Step::Text(text) => *text = text.trim().to_uppercase(),
            Step::Print { .. } | Step::Start | Step::End => {}
        }
        Ok::<_, ()>(())
    };
    template.walk(&mut trim).unwrap();
    let items = Value::List(vec![text("a"), text("b")]);
    let (a, b) = (Value::Bool(true), text("<b>"));
    let context = map(&[("items", items), ("a", a), ("b", b)]);
    let rendered = template.render(&context).unwrap();
    assert_eq!(rendered, "<UL><LI>a</LI><LI>b</LI></UL>É<b>");
    // The tags keep the positions of the source.
    let error = template.render(&map(&[])).unwrap_err();
    assert_eq!((error.pos.line, error.pos.col), (2, 18));
    // The first error ends the walk, whichever of the 16 steps it comes at.
    for last in 1..=16 {
        let mut steps = 0;
        let mut refuse = |_: Pos, _: Step<'_>| {
            steps += 1;
            if steps == last { Err(steps) } else { Ok(()) }
        };
        assert_eq!(template.walk(&mut refuse), Err(last));
        assert_eq!(steps, last);
    }
}

#[test]
fn a_damaged_template_gives_html_or_an_error_never_a_panic() {
    let context = context();
    Template::parse(SAMPLE).unwrap().render(&context).unwrap();
    let sample: Vec<char> = SAMPLE.chars().collect();
    let damage = ['{', '}', '%', '#', '|', '.', ' ', '\n', 'é'];
    // xorshift64 with a fixed seed: every run tries the same 5,000 templates, each with two random
    // characters replaced, inserted or removed.
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
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
        let _ = Template::parse(&text).and_then(|mut template| {
            let walked = template.walk(&mut |_, _| Ok::<_, ()>(()));
            assert_eq!(walked, Ok(()));
            template.render(&context)
        });
    }
}
