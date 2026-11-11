//! The files of `content/` and `static/` that are not pages: only images, PDF and text files,
//! LaTeX files and stylesheets. The build copies them as they are, but for a stylesheet, which it
//! writes as `edu-minify` gives it, without its comments. A browser runs the scripts of other
//! types of file, such as HTML, as if they were pages of the site, and those of an SVG image opened
//! on its own too, so the site publishes an SVG image only if the scan of `svg.rs` lets it through.

use std::fmt;
use std::fs;
use std::path::Path;

use crate::Error;
use crate::check::{ContentError, PageError};
use crate::page::TOP;
use crate::svg::{self, SvgError};

/// The extensions, in lowercase, of the files that the site publishes besides its pages.
const PUBLISHED: [&str; 14] = [
    "avif", "bib", "cls", "css", "gif", "ico", "jpeg", "jpg", "pdf", "png", "svg", "tex", "txt",
    "webp",
];

/// The file that names the site's domain for GitHub Pages, the one file without an extension.
const CNAME: &str = "static/CNAME";

/// Why the site does not publish a file of `content/` or `static/` that is not a page.
#[derive(Debug, PartialEq, Eq)]
pub enum FileError {
    /// An HTML file, whose scripts a browser would run as a page of the site.
    Html,
    /// A file with another extension than those the site publishes, in lowercase, or with none.
    Type(Option<String>),
    /// An SVG image that the scan refuses.
    Svg(SvgError),
    /// A stylesheet that is not UTF-8, as the build reads it and as the site serves it.
    NotUtf8,
}

/// Checks the file at `path`, relative to `root`, which the build would publish: its type, and the
/// text of an SVG image or of a stylesheet. An error in the file goes to `errors`; a stylesheet
/// without one comes back as the build writes it.
pub(crate) fn check(
    root: &Path,
    path: &Path,
    errors: &mut Vec<ContentError>,
) -> Result<Option<String>, Error> {
    if path == Path::new(CNAME) {
        return Ok(None);
    }
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    let error = match extension.as_deref() {
        Some("svg") => match text(root, path)?.map(|text| svg::scan(&text)) {
            Some(Ok(())) => return Ok(None),
            Some(Err((pos, error))) => PageError::File(pos, FileError::Svg(error)),
            None => PageError::File(TOP, FileError::Svg(SvgError::NotText)),
        },
        Some("css") => match text(root, path)?.map(|text| edu_minify::css(&text)) {
            Some(Ok(css)) => return Ok(Some(css)),
            Some(Err(error)) => PageError::Style(error),
            None => PageError::File(TOP, FileError::NotUtf8),
        },
        Some(extension) if PUBLISHED.contains(&extension) => return Ok(None),
        Some("htm" | "html" | "xhtml") => PageError::File(TOP, FileError::Html),
        _ => PageError::File(TOP, FileError::Type(extension)),
    };
    let path = path.to_owned();
    errors.push(ContentError { path, error });
    Ok(None)
}

/// The text of the file at `path`, relative to `root`, or `None` if the file is not UTF-8.
fn text(root: &Path, path: &Path) -> Result<Option<String>, Error> {
    let read_error = |source| Error::Read {
        path: path.to_owned(),
        source,
    };
    let bytes = fs::read(root.join(path)).map_err(read_error)?;
    Ok(String::from_utf8(bytes).ok())
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Html => f.write_str(
                "the site publishes no HTML files; write the page in Markdown, as a `.md` file",
            ),
            Self::Type(extension) => {
                match extension {
                    Some(extension) => write!(f, "the site publishes no `.{extension}` files")?,
                    None => f.write_str("the site publishes no files without an extension")?,
                }
                f.write_str("; besides its pages, it publishes ")?;
                let last = PUBLISHED.len() - 1;
                for (at, extension) in PUBLISHED.iter().enumerate() {
                    let joint = match at {
                        0 => "",
                        _ if at == last => " and ",
                        _ => ", ",
                    };
                    write!(f, "{joint}`.{extension}`")?;
                }
                f.write_str(" files")
            }
            Self::Svg(error) => error.fmt(f),
            Self::NotUtf8 => f.write_str("a stylesheet is UTF-8 text; save it as UTF-8"),
        }
    }
}
