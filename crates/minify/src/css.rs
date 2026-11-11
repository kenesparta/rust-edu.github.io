//! The pass over the tokens of a stylesheet. Every token is written as its bytes in the
//! stylesheet, and the pass decides only what stays of the white space before it: nothing, one
//! character of it, or all of it. One character is the answer wherever the pass has no other,
//! which no CSS grammar can tell from the white space as written. Nothing is the answer only
//! where one of the two tokens is `{`, `}`, `;`, `,` or the `:` of a declaration, none of which
//! can join another token, and where CSS itself drops the white space. So a selector and what
//! follows `@media` or `@supports` are runs of tokens that the pass does not read: it only counts
//! their brackets, to know which `{` starts the block.

use std::mem;

use crate::ErrorKind;
use crate::token::{Bracket, Fail, Kind, Part, Token};

/// The at-rules that the pass knows, and what the block of each holds.
const AT_RULES: [(&str, Holds); 2] = [("@media", Holds::Rules), ("@supports", Holds::Rules)];

/// What a block holds.
#[derive(Clone, Copy)]
enum Holds {
    Rules,
    Declarations,
}

/// Where the pass is in the stylesheet.
#[derive(Clone, Copy)]
enum Context {
    /// Between two rules or two declarations, whichever the innermost open block holds.
    Between,
    /// In a selector, or in what follows the name of an at-rule, before a block that holds this.
    Prelude(Holds),
    /// After the name of a property, before its `:`.
    Name(Property),
    /// In the value of a property.
    Value(Property),
}

/// What a declaration sets.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Property {
    /// A property of CSS, such as `color`, whose value has CSS's grammar of values.
    Plain,
    /// A custom property, `--name`, whose value a script reads back as it is written.
    Custom,
}

/// What stays of the white space between two tokens.
#[derive(Clone, Copy)]
enum Gap {
    Nothing,
    /// One character of it: a space, when it has one.
    Space,
    /// All of it.
    AsWritten,
}

/// The stylesheet `text`, whose tokens are `tokens`, without the white space that no token
/// needs, without its comments and without the `;` that ends no declaration before another.
pub(crate) fn minify(text: &str, tokens: &[Token]) -> Result<String, Fail> {
    let mut pass = Pass {
        text,
        out: String::with_capacity(text.len()),
        context: Context::Between,
        blocks: Vec::new(),
        brackets: Vec::new(),
        start: 0,
        last: None,
        end: 0,
        semicolon: false,
    };
    for token in tokens {
        pass.token(token)?;
        pass.end = token.end;
    }
    Ok(pass.out)
}

/// The state of the pass: what is written so far, and where the next token stands.
struct Pass<'a> {
    text: &'a str,
    out: String,
    context: Context,
    /// The blocks that are open, innermost last: what each holds and where its `{` is.
    blocks: Vec<(Holds, usize)>,
    /// The `(`, `[` and functions open in the prelude or value at hand, each with where it is.
    brackets: Vec<(Bracket, usize)>,
    /// Where the rule or the declaration at hand starts.
    start: usize,
    /// The token before, when it is of the same prelude or value.
    last: Option<Part>,
    /// Where the token before ends.
    end: usize,
    /// Whether a declaration ended with a `;` that is not written yet: it is written only before
    /// another declaration of the same block.
    semicolon: bool,
}

impl Pass<'_> {
    /// Writes `token`, or what stays of it and of the white space before it.
    fn token(&mut self, token: &Token) -> Result<(), Fail> {
        let comment = match self.context {
            Context::Between => None,
            Context::Prelude(_) | Context::Name(_) | Context::Value(_) => token.comment,
        };
        // A browser gives the value of a custom property and some preludes to a script with
        // their comments, and a comment may be all that parts two tokens.
        if let Some(comment) = comment {
            return Err((comment, ErrorKind::CommentInside));
        }
        match self.context {
            Context::Between => match self.blocks.last() {
                Some((Holds::Declarations, _)) => self.declaration(token),
                Some((Holds::Rules, _)) | None => self.rule(token),
            },
            Context::Prelude(holds) => self.prelude(token, holds),
            Context::Name(property) => self.name(token, property),
            Context::Value(property) => self.value(token, property),
        }
    }

    /// A token where a rule can start: its first, a `}` or the end.
    fn rule(&mut self, token: &Token) -> Result<(), Fail> {
        let holds = match token.kind {
            Kind::End => return self.end(),
            Kind::Close => return self.close(token),
            Kind::Semicolon => return Err((token.start, ErrorKind::StraySemicolon)),
            Kind::Open => Holds::Declarations,
            Kind::Part(part) => self.holds(token, part)?,
        };
        self.start = token.start;
        self.last = None;
        self.context = Context::Prelude(holds);
        self.prelude(token, holds)
    }

    /// What the block of the rule that starts with `token` holds: declarations after a selector,
    /// and after the name of an at-rule what [`AT_RULES`] says.
    fn holds(&self, token: &Token, part: Part) -> Result<Holds, Fail> {
        if part != Part::AtKeyword {
            return Ok(Holds::Declarations);
        }
        let name = self.text.get(token.start..token.end).unwrap_or_default();
        for (known, holds) in AT_RULES {
            if name.eq_ignore_ascii_case(known) {
                return Ok(holds);
            }
        }
        Err((token.start, ErrorKind::UnknownAtRule(name.to_owned())))
    }

    /// A token of a prelude, or the `{` of its block, which holds `holds`.
    fn prelude(&mut self, token: &Token, holds: Holds) -> Result<(), Fail> {
        match token.kind {
            Kind::Part(part) => {
                let gap = self.last.map_or(Gap::Nothing, |last| between(last, part));
                self.part(token, part, gap)
            }
            Kind::Open => {
                self.closed()?;
                self.blocks.push((holds, token.start));
                self.out.push('{');
                self.context = Context::Between;
                Ok(())
            }
            Kind::Semicolon | Kind::Close | Kind::End => Err((self.start, ErrorKind::NoBlock)),
        }
    }

    /// A token where a declaration can start: the name of a property, a `;`, a `}` or the end.
    fn declaration(&mut self, token: &Token) -> Result<(), Fail> {
        match token.kind {
            Kind::End => self.end(),
            Kind::Close => self.close(token),
            // A declaration of nothing.
            Kind::Semicolon => Ok(()),
            Kind::Open => Err((token.start, ErrorKind::NotDeclaration)),
            Kind::Part(part) => self.property(token, part),
        }
    }

    /// The first token of a declaration: the name of its property.
    fn property(&mut self, token: &Token, part: Part) -> Result<(), Fail> {
        if part != Part::Ident {
            return Err((token.start, ErrorKind::NotDeclaration));
        }
        if mem::take(&mut self.semicolon) {
            self.out.push(';');
        }
        let name = self.text.get(token.start..token.end).unwrap_or_default();
        self.out.push_str(name);
        self.start = token.start;
        let property = match name.strip_prefix("--") {
            Some(_) => Property::Custom,
            None => Property::Plain,
        };
        self.context = Context::Name(property);
        Ok(())
    }

    /// The token after the name of a property: its `:`.
    fn name(&mut self, token: &Token, property: Property) -> Result<(), Fail> {
        if token.kind != Kind::Part(Part::Colon) {
            return Err((self.start, ErrorKind::NotDeclaration));
        }
        self.out.push(':');
        self.last = None;
        self.context = Context::Value(property);
        Ok(())
    }

    /// A token of the value of a property, or the `;` or `}` that ends it.
    fn value(&mut self, token: &Token, property: Property) -> Result<(), Fail> {
        match token.kind {
            Kind::Part(part) => {
                let gap = match (self.last, property) {
                    (None, Property::Plain | Property::Custom) => Gap::Nothing,
                    (Some(last), Property::Plain) => between(last, part),
                    (Some(_), Property::Custom) => Gap::AsWritten,
                };
                self.part(token, part, gap)
            }
            Kind::Semicolon | Kind::Close => {
                self.closed()?;
                // `--x: ;` keeps a white space: browsers before 2021 refuse `--x:;`.
                if self.last.is_none() && property == Property::Custom {
                    self.out.extend(token.space);
                }
                if token.kind == Kind::Close {
                    return self.close(token);
                }
                self.semicolon = true;
                self.context = Context::Between;
                Ok(())
            }
            Kind::Open => Err((token.start, ErrorKind::Nested)),
            Kind::End => self.end(),
        }
    }

    /// Writes `token`, a part of a prelude or of a value, after what stays of the white space
    /// before it, and counts it if it is a bracket.
    fn part(&mut self, token: &Token, part: Part, gap: Gap) -> Result<(), Fail> {
        match part {
            Part::Function | Part::Open(Bracket::Round) => {
                self.brackets.push((Bracket::Round, token.start));
            }
            Part::Open(Bracket::Square) => self.brackets.push((Bracket::Square, token.start)),
            Part::Close(bracket) => {
                if self.brackets.pop().map(|(open, _)| open) != Some(bracket) {
                    return Err((token.start, ErrorKind::Unbalanced(bracket.close())));
                }
            }
            Part::Ident
            | Part::AtKeyword
            | Part::Colon
            | Part::Comma
            | Part::Sign
            | Part::Other => {}
        }
        match gap {
            Gap::Nothing => {}
            Gap::Space => self.out.extend(token.space),
            Gap::AsWritten => {
                let gap = self.text.get(self.end..token.start).unwrap_or_default();
                self.out.push_str(gap);
            }
        }
        let written = self.text.get(token.start..token.end).unwrap_or_default();
        self.out.push_str(written);
        self.last = Some(part);
        Ok(())
    }

    /// Checks that no `(`, `[` or function of the prelude or value at hand is still open.
    fn closed(&self) -> Result<(), Fail> {
        match self.brackets.last() {
            Some(&(bracket, at)) => Err((at, ErrorKind::Unclosed(bracket.close()))),
            None => Ok(()),
        }
    }

    /// A `}`, which ends the innermost block.
    fn close(&mut self, token: &Token) -> Result<(), Fail> {
        if self.blocks.pop().is_none() {
            return Err((token.start, ErrorKind::Unbalanced('}')));
        }
        self.out.push('}');
        // The last `;` of a block ends no declaration before another.
        self.semicolon = false;
        self.context = Context::Between;
        Ok(())
    }

    /// The end of the stylesheet, where nothing may be open.
    fn end(&self) -> Result<(), Fail> {
        self.closed()?;
        match self.blocks.last() {
            Some(&(_, at)) => Err((at, ErrorKind::Unclosed('}'))),
            None => Ok(()),
        }
    }
}

/// What stays of the white space between `last` and `next`, two tokens of one selector, of one
/// prelude of an at-rule or of one value: nothing beside a comma, where every list of CSS allows
/// white space and needs none, and one character anywhere else.
fn between(last: Part, next: Part) -> Gap {
    let comma = last == Part::Comma || next == Part::Comma;
    // `calc()` reads `+` and `-` only with white space on both sides, so none beside one goes.
    let sign = last == Part::Sign || next == Part::Sign;
    // An argument of white space alone, as in `var(--x, )`: browsers before 2021 refuse
    // `var(--x,)`.
    let opens = matches!(last, Part::Comma | Part::Function | Part::Open(_));
    let empty = opens && matches!(next, Part::Comma | Part::Close(_));
    if comma && !sign && !empty {
        Gap::Nothing
    } else {
        Gap::Space
    }
}
