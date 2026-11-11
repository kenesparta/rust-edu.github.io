//! The scan of a layout's markup, which comes in runs: the layout's own text between its template
//! tags. At each character the scan is in text or in a tag, and it keeps that place from one run
//! to the next, since a template tag often stands inside a tag. It writes a run of white space
//! with a line break in it as one line break, and stops at what could put a browser's parser in
//! another place than its own.

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

/// The elements that the scan does not read yet: `pre`, which shows its white space as written,
/// and those whose text an HTML parser does not read as markup.
const UNREAD: [&str; 12] = [
    "pre",
    "script",
    "style",
    "title",
    "textarea",
    "iframe",
    "noscript",
    "xmp",
    "listing",
    "plaintext",
    "noembed",
    "noframes",
];

/// The scan of the markup of one layout, from `Markup::default()` on. A layout's own text comes
/// to [`Markup::text`] run by run, in the order of the source: two runs in a row are neighbours
/// in every page, as around a comment of the template language. [`Markup::finish`] ends the scan,
/// and so does its first error.
///
/// A run of white space with a line break in it is written as one line break, in text between
/// tags and between the attributes of a tag. Everything else is written as it is: the other white
/// space, and each value of an attribute.
#[derive(Debug, Default)]
pub struct Markup {
    /// Where the scan is.
    place: Place,
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
    /// Right after the `=` of an attribute.
    Equals,
    /// The value of an attribute, in its double quotes.
    Value,
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

    /// Ends the scan at the end of the layout, where no tag is open.
    pub fn finish(self) -> Result<(), Error> {
        let (pos, kind) = match self.place {
            Place::Text => return Ok(()),
            Place::Tag(tag) => match tag.part {
                Part::Open | Part::Declaration => (tag.open, ErrorKind::LessThan),
                Part::Name | Part::Attributes | Part::Equals | Part::Value => {
                    (tag.open, ErrorKind::UnclosedTag)
                }
            },
        };
        Err(Error { pos, kind })
    }

    /// Reads the character `c` at `pos`, and writes it to `out` unless it is white space that a
    /// line break stands for.
    fn read(&mut self, pos: Pos, c: char, out: &mut String) -> Result<(), Error> {
        let in_run = self.place.shortens() && BLANKS.contains(&c);
        self.place = match mem::take(&mut self.place) {
            Place::Text if c == '<' => Place::Tag(Tag::new(pos)),
            Place::Text => Place::Text,
            Place::Tag(tag) => self.tag(tag, pos, c)?,
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
            Part::Name | Part::Attributes if c == '>' => return self.close(tag),
            Part::Name | Part::Attributes => match c {
                '<' => return refused(tag.open, ErrorKind::UnclosedTag),
                // An HTML parser takes a `=` with no name before it for the start of a name, and
                // would end the tag at a `>` in what the scan takes for a value.
                '=' if tag.part == Part::Attributes && tag.attribute.is_empty() => {
                    return refused(pos, ErrorKind::UnquotedValue);
                }
                '=' if tag.part == Part::Attributes => {
                    tag.attribute.clear();
                    tag.part = Part::Equals;
                }
                _ if c == '/' || is_space(c) => {
                    tag.attribute.clear();
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

    /// The place after the `>` of `tag`: text between tags, unless `tag` starts an element that
    /// the scan does not read yet.
    fn close(&mut self, tag: Tag) -> Result<Place, Error> {
        if !tag.end
            && UNREAD
                .iter()
                .any(|name| tag.name.eq_ignore_ascii_case(name))
        {
            let (pos, kind) = (tag.open, ErrorKind::UnreadElement);
            return Err(Error { pos, kind });
        }
        Ok(Place::Text)
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
    /// text, and in a tag outside the values of its attributes.
    fn shortens(&self) -> bool {
        match self {
            Self::Text => true,
            Self::Tag(tag) => tag.part != Part::Value,
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
        }
    }
}

impl Default for Run {
    fn default() -> Self {
        Self::Blanks(0)
    }
}
