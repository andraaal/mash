use crate::args::{Args, Token};
use crate::cmd::Cmd;
use crate::{ShellState, MAX_ALIAS_DEPTH};
use std::iter::Peekable;

/// Pratt parser that turns tokenized shell input into an AST.
///
/// The parser also resolves aliases before building each command expression.
pub(crate) struct Parser<'a> {
    expressions: Vec<Expr>,
    errors: Vec<String>,
    state: &'a ShellState,
    queue: Vec<Peekable<Args<'a>>>,
}

impl<'a> Parser<'_> {
    /// Create a new Parser from Args and a ShellState.
    pub fn new(args: Peekable<Args<'a>>, state: &'a ShellState) -> Parser<'a> {
        Parser {
            expressions: Vec::new(),
            errors: Vec::new(),
            state,
            queue: vec![args],
        }
    }

    /// Parses the input stream into expressions.
    ///
    /// On success, returns the parsed expressions. On failure, returns every
    /// syntax error collected while continuing to parse the rest of the line.
    pub fn compile(mut self) -> Result<Vec<Expr>, Vec<String>> {
        let mut next = self.expression();
        self.expressions.push(next);
        while self.peek_token().is_some() {
            if !self.consume(|tk| match tk {
                Token::Symbol(sym) => sym == "\n",
                _ => false,
            }) {
                let err = self.error(
                    "Unexpected start of expression, expected newline or expression seperator"
                        .to_string(),
                );
                self.expressions.push(err);
                break;
            }
            next = self.expression();
            self.expressions.push(next);
        }
        if self.errors.is_empty() {
            Ok(self.expressions)
        } else {
            Err(self.errors)
        }
    }

    fn expression(&mut self) -> Expr {
        self.parse_precedence(0)
    }

    fn parse_precedence(&mut self, min_prec: u32) -> Expr {
        if let Some(prefix_tk) = self.next_token_aliased() {
            let mut lhs;
            if let Some(parselet) = Self::prefix_parselet(&prefix_tk) {
                lhs = (parselet.parse)(self, prefix_tk);
            } else {
                return self.error(format!("Invalid start of expression: {:?}", prefix_tk));
            }

            while let Some(infix_tk) = self.peek_token() {
                if let Some(parselet) = Self::infix_parselet(infix_tk)
                    && parselet.precedence > min_prec
                {
                    let tk = self.next_token_aliased().unwrap();
                    lhs = (parselet.parse)(self, tk, lhs);
                } else {
                    break;
                }
            }
            lhs
        } else {
            self.error("Invalid start of expression: EOF".to_string())
        }
    }

    fn next_token(&mut self) -> Option<Token> {
        self.trim_queue();
        let args = self.queue.last_mut()?;
        args.next()
    }

    fn trim_queue(&mut self) {
        while self
            .queue
            .last_mut()
            .is_some_and(|last| last.peek().is_none())
        {
            self.queue.pop();
        }
    }

    fn next_token_aliased(&mut self) -> Option<Token> {
        self.trim_queue();
        let args = self.queue.last_mut()?;
        let next = args.next();
        match next {
            Some(Token::Symbol(sym)) if self.queue.len() <= MAX_ALIAS_DEPTH => {
                for (alias, replacement) in self.state.aliases.iter() {
                    if sym == *alias {
                        self.queue.push(Args::new(replacement).peekable());
                        return self.next_token_aliased();
                    }
                }
                Some(Token::Symbol(sym))
            }
            Some(Token::Symbol(_)) => {
                eprintln!("Max recursion depth reached; results may be unexpected");
                next
            }
            _ => next,
        }
    }
    fn peek_token(&mut self) -> Option<&mut Token> {
        self.trim_queue();
        let args = self.queue.last_mut()?;
        args.peek_mut()
    }

    fn consume(&mut self, condition: fn(&mut Token) -> bool) -> bool {
        if self.peek_token().is_some_and(condition) {
            self.next_token();
            true
        } else {
            false
        }
    }

    fn consume_symbol(&mut self) -> Option<String> {
        if let Some(Token::Symbol(symbol)) = self.peek_token() {
            let res = Some(std::mem::take(symbol));
            self.next_token();
            res
        } else {
            None
        }
    }

    fn error(&mut self, msg: String) -> Expr {
        self.errors.push(msg);
        Expr::Error
    }

    // Any token that cannot start a new expression is treated as an infix operator.
    const fn infix_parselet(tk: &Token) -> Option<InfixParselet> {
        let tp = match tk {
            Token::OverwriteOutToFile => InfixParselet {
                precedence: 10,
                parse: |parser, _token, lhs| {
                    let rhs = parser.consume_symbol();
                    if let Some(right) = rhs {
                        Expr::OverwriteOutToFile(Box::new(lhs), right)
                    } else {
                        parser.error("Expected filename after redirect".to_string())
                    }
                },
            },
            Token::AppendOutToFile => InfixParselet {
                precedence: 10,
                parse: |parser, _token, lhs| {
                    let rhs = parser.consume_symbol();
                    if let Some(right) = rhs {
                        Expr::AppendOutToFile(Box::new(lhs), right)
                    } else {
                        parser.error("Expected filename after redirect".to_string())
                    }
                },
            },
            Token::OverwriteErrToFile => InfixParselet {
                precedence: 10,
                parse: |parser, _token, lhs| {
                    let rhs = parser.consume_symbol();
                    if let Some(right) = rhs {
                        Expr::OverwriteErrToFile(Box::new(lhs), right)
                    } else {
                        parser.error("Expected filename after redirect".to_string())
                    }
                },
            },
            Token::AppendErrToFile => InfixParselet {
                precedence: 10,
                parse: |parser, _token, lhs| {
                    let rhs = parser.consume_symbol();
                    if let Some(right) = rhs {
                        Expr::AppendErrToFile(Box::new(lhs), right)
                    } else {
                        parser.error("Expected filename after redirect".to_string())
                    }
                },
            },
            Token::Pipe => InfixParselet {
                precedence: 5,
                parse: |parser, _token, lhs| {
                    let rhs = parser.parse_precedence(5);
                    Expr::Pipe(Box::new(lhs), Box::new(rhs))
                },
            },
            _ => return None,
        };
        Some(tp)
    }

    ///Any token that can begin an expression is treated as a prefix parselet.
    const fn prefix_parselet(tk: &Token) -> Option<PrefixParselet> {
        let tp = match tk {
            Token::Symbol(_) => PrefixParselet {
                precedence: 10,
                parse: |parser, token| {
                    let mut arguments = Vec::new();
                    while let Some(arg) = parser.consume_symbol() {
                        arguments.push(arg);
                    }
                    let mut command = Cmd::new(&token.as_text());
                    command.set_args(&mut arguments);
                    Expr::Cmd(command)
                },
            },
            _ => return None,
        };
        Some(tp)
    }
}

type Precedence = u32;
struct PrefixParselet {
    #[expect(dead_code)]
    precedence: Precedence,
    parse: fn(parser: &mut Parser, token: Token) -> Expr,
}

struct InfixParselet {
    precedence: Precedence,
    parse: fn(parser: &mut Parser, token: Token, lhs: Expr) -> Expr,
}

/// A shell expression node.
///
/// This covers a command invocation, redirection, piping, or a parser error
/// placeholder used to keep recovering after malformed input.
pub(crate) enum Expr {
    Cmd(Cmd),
    OverwriteOutToFile(Box<Expr>, String),
    AppendOutToFile(Box<Expr>, String),
    OverwriteErrToFile(Box<Expr>, String),
    AppendErrToFile(Box<Expr>, String),
    Pipe(Box<Expr>, Box<Expr>),
    Error, // Error is just here to be able to return something to let the parser continue and find more errors.
}
