//! A syntax message written with the parts it quotes of the text: each
//! built-in language builds its errors here, so every quote is recorded
//! where it is written into the message, never found in it afterwards.

use std::ops::Range;

use super::{ParseError, Quote};

/// A message being written, and its quotes so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Said {
    text: String,
    quotes: Vec<Quote>,
}

impl Said {
    /// `words` after the message so far: the language's own, quoting
    /// nothing of the text.
    pub(crate) fn words(mut self, words: &str) -> Self {
        self.text.push_str(words);
        self
    }

    /// `said` after the message so far, quoting the text's bytes `of`.
    pub(crate) fn quote(mut self, said: &str, of: Range<usize>) -> Self {
        let start = self.text.len();
        self.text.push_str(said);
        self.quotes.push(Quote {
            said: start..self.text.len(),
            of,
        });
        self
    }

    /// `message`, a parser's own sentence that may name the text it
    /// stopped in (its wording is its crate's): one quote of all of it,
    /// of the text from `from` on, so it is hidden whole wherever a secret
    /// may be there.
    pub(crate) fn naming(source: &str, from: usize, message: &str) -> Self {
        Said::default().quote(message, from.min(source.len())..source.len())
    }

    /// The message so far.
    #[cfg_attr(
        not(any(feature = "yaml", feature = "json", feature = "toml", feature = "hcl")),
        expect(
            dead_code,
            reason = "only a language reading numbers reads a message back"
        )
    )]
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The error with this message at `line` and `column`.
    pub(crate) fn at(self, line: Option<usize>, column: Option<usize>) -> ParseError {
        ParseError {
            message: self.text,
            line,
            column,
            quotes: Some(self.quotes),
        }
    }
}

impl From<&str> for Said {
    fn from(words: &str) -> Self {
        Said::default().words(words)
    }
}

impl From<String> for Said {
    fn from(words: String) -> Self {
        Said {
            text: words,
            quotes: Vec::new(),
        }
    }
}

impl ParseError {
    /// This error with each quote's text range moved by `map` (a reading
    /// of another text placed back in the one given).
    pub(crate) fn quotes_mapped(mut self, map: impl Fn(usize) -> usize) -> Self {
        for quote in self.quotes.iter_mut().flatten() {
            quote.of = map(quote.of.start)..map(quote.of.end);
        }
        self
    }
}
