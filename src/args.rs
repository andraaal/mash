use crate::args::Token::{
    AppendErrToFile, AppendOutToFile, OverwriteErrToFile, OverwriteOutToFile, Symbol,
};

pub(crate) struct Args<'a> {
    raw: &'a str,
    pos: usize,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Token {
    Symbol(String),
    OverwriteOutToFile,
    OverwriteErrToFile,
    AppendOutToFile,
    AppendErrToFile,
    Pipe,
}

impl Token {
    pub(crate) fn as_text(&self) -> String {
        match self {
            Symbol(s) => s.clone(),
            OverwriteOutToFile => ">".to_string(),
            Token::Pipe => "|".to_string(),
            Token::OverwriteErrToFile => "2>".to_string(),
            Token::AppendOutToFile => ">>".to_string(),
            Token::AppendErrToFile => "2>>".to_string(),
        }
    }
}

impl<'a> Iterator for Args<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

impl<'a> Args<'a> {
    pub(crate) fn new(raw: &'a str) -> Self {
        Self { raw, pos: 0 }
    }

    fn next_token(&mut self) -> Option<Token> {
        let bytes = self.raw.as_bytes();
        let len = bytes.len();

        let mut start = None;
        let mut quotes = None;
        let mut segments = String::new();

        while self.pos < len {
            let b = bytes[self.pos];

            match b {
                b'\\' if quotes != Some(b'\'') => {
                    if self.pos + 1 < len {
                        let n = bytes[self.pos + 1];
                        // Escape everything outside of quotes; escape only certain characters inside double quotes
                        if quotes.is_none()
                            || n == b'\\'
                            || n == b'"'
                            || n == b'$'
                            || n == b'\n'
                            || n == b'`'
                        {
                            let token = &self.raw[start.unwrap_or(self.pos)..self.pos];
                            segments.push_str(token);
                            self.pos += 1;
                            start = Some(self.pos);
                        }
                    }
                }
                b'\'' | b'"' => {
                    if quotes.is_none() {
                        // opening quote; remember token type
                        quotes = Some(b);
                        let token = &self.raw[start.unwrap_or(self.pos)..self.pos];
                        segments.push_str(token);
                        start = Some(len.min(self.pos + 1));
                    } else if quotes == Some(b) {
                        // closing quote
                        let token = &self.raw[start.unwrap()..self.pos];
                        segments.push_str(token);
                        start = None;

                        if self.pos + 1 < len && Some(bytes[self.pos + 1]) == quotes {
                            self.pos += 1;
                        } else {
                            quotes = None;
                        }
                    }
                }

                b if b.is_ascii_whitespace() && quotes.is_none() => {
                    if let Some(start) = start {
                        let token = &self.raw[start..self.pos];
                        segments.push_str(token);
                        return Some(Symbol(segments));
                    } else if !segments.is_empty() {
                        return Some(Symbol(segments));
                    }
                }

                _ => {
                    if start.is_none() {
                        start = Some(self.pos);

                        // check bash syntax here
                        // having neither a start nor segments implies no quotes
                        if segments.is_empty() {
                            match b {
                                b'>' => {
                                    return if bytes.get(self.pos + 1) == Some(&b'>') {
                                        self.pos += 2;
                                        Some(AppendOutToFile)
                                    } else {
                                        self.pos += 1;
                                        Some(OverwriteOutToFile)
                                    };
                                }
                                b'1' if bytes.get(self.pos + 1) == Some(&b'>') => {
                                    return if bytes.get(self.pos + 2) == Some(&b'>') {
                                        self.pos += 3;
                                        Some(AppendOutToFile)
                                    } else {
                                        self.pos += 2;
                                        Some(OverwriteOutToFile)
                                    };
                                }
                                b'2' if bytes.get(self.pos + 1) == Some(&b'>') => {
                                    return if bytes.get(self.pos + 2) == Some(&b'>') {
                                        self.pos += 3;
                                        Some(AppendErrToFile)
                                    } else {
                                        self.pos += 2;
                                        Some(OverwriteErrToFile)
                                    };
                                }
                                b'|' => {
                                    self.pos += 1;
                                    return Some(Token::Pipe);
                                }

                                _ => {}
                            }
                        }
                    }
                }
            }

            self.pos += 1;
        }

        if let Some(start) = start {
            segments.push_str(&self.raw[start..len]);
        };
        if segments.is_empty() {
            None
        } else {
            Some(Symbol(segments))
        }
    }
}
