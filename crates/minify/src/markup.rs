//! The scan of a layout's markup, which comes in runs: the layout's own text between its template
//! tags. At each character the scan is in text, in a tag, or in the text of an element that an
//! HTML parser does not read as markup, and it keeps that place from one run to the next, since a
//! template tag often stands inside a tag. It writes a run of white space with a line break in it
//! as one line break, and stops at what could put a browser's parser in another place than its
//! own.

use std::mem;

use crate::{Error, ErrorKind, Pos};

/// The white space that a run is made of: a space, a tab, a line feed and a carriage return. A
/// space outside ASCII, such as U+00A0, is text that a reader sees.
const BLANKS: [char; 4] = [' ', '\t', '\n', '\r'];

/// A form feed, which parts the attributes of a tag as a blank does. In text a browser shows it
/// as it would a letter, so it is part of no run.
const FORM_FEED: char = '\u{c}';

/// What follows `<!` in the one declaration that the scan lets through, in any letter case.
const DOCTYPE: &str = "doctype html";

/// What starts an HTML comment.
const COMMENT: &str = "<!--";

/// The scan of the markup of one layout, from `Markup::default()` on. A layout's own text comes
/// to [`Markup::text`] run by run, in the order of the source: two runs in a row are neighbours
/// in every page, as around a comment of the template language. [`Markup::finish`] ends the scan,
/// and so does its first error.
///
/// A run of white space with a line break in it is written as one line break: in text between
/// tags, between the attributes of a tag, and in a script without a `type`. Everything else is
/// written as it is: the other white space, each value of an attribute, what is in `pre`, and the
/// text of a script with a `type` and of the other elements whose text an HTML parser does not
/// read as markup, such as `textarea`, `title` and `style`.
#[derive(Debug, Default)]
pub struct Markup {
    /// Where the scan is.
    place: Place,
    /// Where the `<` of each `pre` that is still open is.
    pre: Vec<Pos>,
    /// Where the `<` of each `svg` and `math` that is still open is. A browser reads what they
    /// hold by other rules than those of HTML: every `<` is markup there.
    foreign: Vec<Pos>,
    /// The white space that the scan is in.
    run: Run,
}

/// Where the scan is in the markup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Place {
    /// In text between tags.
    #[default]
    Text,
    /// In a tag, or in the `<!DOCTYPE html>` of a page.
    Tag(Tag),
    /// In the text of an element that is no markup.
    Kept(Kept),
}

/// A tag that the scan is in.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Tag {
    /// Where its `<` is.
    open: Pos,
    /// The part of it that the scan is in.
    part: Part,
    /// Whether it is an end tag.
    end: bool,
    /// The name of its element as far as read, or after `<!` the text that follows.
    name: String,
    /// The name of the attribute that the scan is in, as far as read.
    attribute: String,
    /// Whether it has an attribute `type`.
    typed: bool,
}

/// The parts of a tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// Right after `<` or `</`.
    Open,
    /// After `<!`.
    Declaration,
    /// The name of the element.
    Name,
    /// After the name: the attributes, and what parts them.
    Attributes,
    /// Right after a `/` that parts attributes. Before the `>` of a start tag it ends an element
    /// of an SVG image, and does nothing in HTML.
    Slash,
    /// Right after the `=` of an attribute.
    Equals,
    /// The value of an attribute, in its double quotes.
    Value,
}

/// An element whose text an HTML parser does not read as markup, in which the scan is.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Kept {
    /// Where the `<` of its start tag is.
    start: Pos,
    /// How the scan reads its text.
    reading: Reading,
    /// What starts its end tag, in lowercase, such as `</title`.
    end: String,
    /// Where the last `<` of its text is and the text from there on, while that may still become
    /// its end tag or, in a script, `<!--`.
    tail: Option<(Pos, String)>,
    /// Where the `\` of a script is that only white space parts from the scan.
    escape: Option<Pos>,
}

/// How the scan reads the text of an element that is no markup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reading {
    /// As it is written: `title`, `textarea`, `noscript`, `style` and `iframe`, and the elements
    /// of old that an HTML parser still reads so, or shows as written: `xmp`, `listing`,
    /// `plaintext`, `noembed`, `noframes`.
    Kept,
    /// As a script with a `type`, which may not be JavaScript.
    Typed,
    /// As JavaScript, where a run of white space with a line break is written as one line break.
    Script,
}

/// The white space that the scan is in, where it writes a run with a line break as one.
#[derive(Clone, Copy, Debug)]
enum Run {
    /// No line break so far, and this many blanks of the run written: none outside a run.
    Blanks(usize),
    /// A run whose line break is written, and whose other blanks are left out.
    Broken,
}

impl Markup {
    /// What `text` is written as: a run of the layout's own text that starts at `start`. The
    /// scan goes on from where the run before it ended. An error has its line and column in the
    /// layout.
    pub fn text(&mut self, start: Pos, text: &str) -> Result<String, Error> {
        let mut out = String::with_capacity(text.len());
        let mut pos = start;
        for c in text.chars() {
            self.read(pos, c, &mut out)?;
            let (line, col) = match c {
                '\n' => (pos.line + 1, 1),
                _ => (pos.line, pos.col + 1),
            };
            pos = Pos { line, col };
        }
        Ok(out)
    }

    /// Ends the scan at the end of the layout, where no tag is open, and neither a `pre`, an
    /// `svg`, a `math` nor an element whose text is no markup.
    pub fn finish(self) -> Result<(), Error> {
        let open = self.pre.first().or(self.foreign.first());
        let (pos, kind) = match (self.place, open) {
            (Place::Text, None) => return Ok(()),
            (Place::Text, Some(open)) => (*open, ErrorKind::UnclosedElement),
            (Place::Tag(tag), _) => match tag.part {
                Part::Open | Part::Declaration => (tag.open, ErrorKind::LessThan),
                Part::Name | Part::Attributes | Part::Slash | Part::Equals | Part::Value => {
                    (tag.open, ErrorKind::UnclosedTag)
                }
            },
            (Place::Kept(kept), _) => (kept.start, ErrorKind::UnclosedElement),
        };
        Err(Error { pos, kind })
    }

    /// Reads the character `c` at `pos`, and writes it to `out` unless it is white space that a
    /// line break stands for.
    fn read(&mut self, pos: Pos, c: char, out: &mut String) -> Result<(), Error> {
        let in_run = self.pre.is_empty() && self.place.shortens() && BLANKS.contains(&c);
        self.place = match mem::take(&mut self.place) {
            Place::Text if c == '<' => Place::Tag(Tag::new(pos)),
            Place::Text => Place::Text,
            Place::Tag(tag) => self.tag(tag, pos, c)?,
            Place::Kept(kept) => self.kept(kept, pos, c)?,
        };
        self.run = match self.run {
            Run::Blanks(written) if in_run && c == '\n' => {
                // The line break takes the place of the blanks before it, as far as they are in
                // this text: those that an earlier text ended with are written, and stay.
                for _ in 0..written {
                    out.pop();
                }
                out.push(c);
                Run::Broken
            }
            Run::Blanks(written) if in_run => {
                out.push(c);
                Run::Blanks(written + 1)
            }
            Run::Broken if in_run => Run::Broken,
            Run::Blanks(_) | Run::Broken => {
                out.push(c);
                Run::default()
            }
        };
        Ok(())
    }

    /// Reads the character `c` at `pos` of `tag`, and gives the place after it. A tag has its
    /// name, then attributes of the form `name="value"` or `name`, parted by white space or `/`,
    /// and then its `>`; where the scan lets other forms through, it reads them as an HTML
    /// parser does.
    fn tag(&mut self, mut tag: Tag, pos: Pos, c: char) -> Result<Place, Error> {
        let refused = |pos, kind| Err(Error { pos, kind });
        match tag.part {
            Part::Open => match c {
                '/' if !tag.end => tag.end = true,
                '!' if !tag.end => tag.part = Part::Declaration,
                _ if c.is_ascii_alphabetic() => {
                    tag.name.push(c);
                    tag.part = Part::Name;
                }
                _ => return refused(tag.open, ErrorKind::LessThan),
            },
            Part::Declaration => {
                if c == '>' && tag.name.eq_ignore_ascii_case(DOCTYPE) {
                    return Ok(Place::Text);
                }
                tag.name.push(c);
                match tag.name.as_str() {
                    "--" => return refused(tag.open, ErrorKind::HtmlComment),
                    "-" => {}
                    name if starts(DOCTYPE, name) => {}
                    _ => return refused(tag.open, ErrorKind::LessThan),
                }
            }
            Part::Name | Part::Attributes | Part::Slash if c == '>' => {
                tag.end_attribute();
                return self.close(tag);
            }
            Part::Slash => {
                tag.part = Part::Attributes;
                return self.tag(tag, pos, c);
            }
            Part::Name | Part::Attributes => match c {
                '<' => return refused(tag.open, ErrorKind::UnclosedTag),
                // An HTML parser takes a `=` with no name before it for the start of a name, and
                // would end the tag at a `>` in what the scan takes for a value.
                '=' if tag.part == Part::Attributes && tag.attribute.is_empty() => {
                    return refused(pos, ErrorKind::UnquotedValue);
                }
                '=' if tag.part == Part::Attributes => {
                    tag.end_attribute();
                    tag.part = Part::Equals;
                }
                '/' => {
                    tag.end_attribute();
                    tag.part = Part::Slash;
                }
                _ if is_space(c) => {
                    tag.end_attribute();
                    tag.part = Part::Attributes;
                }
                _ if tag.part == Part::Name => tag.name.push(c),
                _ => tag.attribute.push(c),
            },
            Part::Equals if c == '"' => tag.part = Part::Value,
            Part::Equals => return refused(pos, ErrorKind::UnquotedValue),
            Part::Value if c == '"' => tag.part = Part::Attributes,
            Part::Value => {}
        }
        Ok(Place::Tag(tag))
    }

    /// The place after the `>` of `tag`: the text of its element, if an HTML parser does not read
    /// that as markup, and else text between tags. A `pre` is open from its start tag to its end
    /// tag, and may hold another, and so are `svg` and `math`.
    fn close(&mut self, tag: Tag) -> Result<Place, Error> {
        let name = tag.name.to_ascii_lowercase();
        let reading = match (name.as_str(), tag.end) {
            ("pre", false) => {
                self.pre.push(tag.open);
                return Ok(Place::Text);
            }
            ("pre", true) => {
                self.pre.pop();
                return Ok(Place::Text);
            }
            ("svg" | "math", false) if tag.part != Part::Slash => {
                self.foreign.push(tag.open);
                return Ok(Place::Text);
            }
            ("svg" | "math", true) => {
                self.foreign.pop();
                return Ok(Place::Text);
            }
            (
                "title" | "textarea" | "noscript" | "style" | "iframe" | "xmp" | "listing"
                | "plaintext" | "noembed" | "noframes",
                false,
            ) => Reading::Kept,
            ("script", false) if tag.typed => Reading::Typed,
            ("script", false) => Reading::Script,
            _ => return Ok(Place::Text),
        };
        // An SVG image has such elements too, and there `/>` ends one, with no text.
        if tag.part == Part::Slash {
            let (pos, kind) = (tag.open, ErrorKind::SelfClosed);
            return Err(Error { pos, kind });
        }
        Ok(Place::Kept(Kept {
            start: tag.open,
            reading,
            end: format!("</{name}"),
            tail: None,
            escape: None,
        }))
    }

    /// Reads the character `c` at `pos` of the text of `kept`, and gives the place after it: the
    /// end tag of `kept` once `</`, its name and then white space, `/` or `>` have come, which is
    /// the only end that an HTML parser knows of such an element, in a string of a script too.
    fn kept(&mut self, mut kept: Kept, pos: Pos, c: char) -> Result<Place, Error> {
        let is_script = match kept.reading {
            Reading::Script | Reading::Typed => true,
            Reading::Kept => false,
        };
        let ends_name = c == '/' || c == '>' || is_space(c);
        match kept.tail.take() {
            Some((open, text)) if ends_name && text.len() == kept.end.len() => {
                let mut tag = Tag::new(open);
                (tag.part, tag.end) = (Part::Name, true);
                tag.name.extend(text.chars().skip(2));
                return self.tag(tag, pos, c);
            }
            Some((open, mut text)) => {
                text.push(c);
                // After `<!--`, an HTML parser may read past the `</script>` that follows.
                let is_comment = is_script && starts(COMMENT, &text);
                if is_comment && text.len() == COMMENT.len() {
                    let kind = ErrorKind::HtmlComment;
                    return Err(Error { pos: open, kind });
                }
                if is_comment || starts(&kept.end, &text) {
                    kept.tail = Some((open, text));
                } else if !self.foreign.is_empty() {
                    // In `svg` and `math`, the text of such an element is markup to a browser.
                    let (pos, kind) = (open, ErrorKind::ForeignMarkup);
                    return Err(Error { pos, kind });
                }
            }
            None => {}
        }
        if c == '<' {
            kept.tail = Some((pos, String::from(c)));
        }
        match kept.reading {
            // In JavaScript only a template literal and a string that a `\` carries on hold a
            // line break with blanks around it that count; every other one parts two tokens, or
            // is in a comment.
            Reading::Script => match c {
                '`' => {
                    let kind = ErrorKind::Backtick;
                    return Err(Error { pos, kind });
                }
                '\\' => kept.escape = Some(pos),
                '\n' => {
                    if let Some(pos) = kept.escape {
                        let kind = ErrorKind::BrokenString;
                        return Err(Error { pos, kind });
                    }
                }
                _ if BLANKS.contains(&c) => {}
                _ => kept.escape = None,
            },
            Reading::Typed | Reading::Kept => {}
        }
        Ok(Place::Kept(kept))
    }
}

/// Whether `c` is white space to an HTML parser.
fn is_space(c: char) -> bool {
    c == FORM_FEED || BLANKS.contains(&c)
}

/// Whether `whole` starts with `start`, in any letter case of ASCII.
fn starts(whole: &str, start: &str) -> bool {
    let head = whole.get(..start.len());
    head.is_some_and(|head| head.eq_ignore_ascii_case(start))
}

impl Place {
    /// Whether a run of white space with a line break is written as one line break here: in
    /// text, in a tag outside the values of its attributes, and in JavaScript.
    fn shortens(&self) -> bool {
        match self {
            Self::Text => true,
            Self::Tag(tag) => tag.part != Part::Value,
            Self::Kept(kept) => kept.reading == Reading::Script,
        }
    }
}

impl Tag {
    /// The tag whose `<` is at `open`.
    fn new(open: Pos) -> Self {
        Self {
            open,
            part: Part::Open,
            end: false,
            name: String::new(),
            attribute: String::new(),
            typed: false,
        }
    }

    /// Ends the name of an attribute, if the scan is in one.
    fn end_attribute(&mut self) {
        self.typed |= self.attribute.eq_ignore_ascii_case("type");
        self.attribute.clear();
    }
}

impl Default for Run {
    fn default() -> Self {
        Self::Blanks(0)
    }
}
