use crate::args::{Args, Token};
use crate::cmd::Cmd;
use std::iter::Peekable;

pub(crate) struct Parser<'a> {
    tokens: Peekable<Args<'a>>,
    expressions: Vec<Expr>,
    errors: Vec<String>,
}

impl<'a> Parser<'_> {
    pub fn new(args: Peekable<Args<'a>>) -> Parser<'a> {
        Parser {
            tokens: args,
            expressions: Vec::new(),
            errors: Vec::new(),
        }
    }
    pub fn compile(mut self) -> Result<Vec<Expr>, Vec<String>> {
        let mut next = self.expression();
        self.expressions.push(next);
        while self.tokens.peek().is_some() {
            if !self.consume(|tk| match tk {
                Token::Symbol(sym) => sym == "\n",
                _ => false,
            }) {
                let err = self.error("Unexpected start of expression, expected newline or expression seperator".to_string());
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
        if let Some(prefix_tk) = self.next_token() {
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
                    let tk = self.next_token().unwrap();
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
        self.tokens.next()
    }
    fn peek_token(&mut self) -> Option<&Token> {
        self.tokens.peek()
    }

    fn consume(&mut self, condition: fn(&Token) -> bool) -> bool {
        if self.tokens.peek().is_some_and(condition) {
            self.tokens.next();
            true
        } else {
            false
        }
    }

    fn consume_symbol(&mut self) -> Option<String> {
        if let Some(Token::Symbol(symbol)) = self.tokens.peek_mut() {
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

    // Any token that can't be at the start of an expression is considered infix
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

    // Any token that can start an expression is considered prefix
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
                    command.set_args(arguments);
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
    precedence: Precedence,
    parse: fn(parser: &mut Parser, token: Token) -> Expr,
}

struct InfixParselet {
    precedence: Precedence,
    parse: fn(parser: &mut Parser, token: Token, lhs: Expr) -> Expr,
}

pub(crate) enum Expr {
    Cmd(Cmd),
    OverwriteOutToFile(Box<Expr>, String),
    AppendOutToFile(Box<Expr>, String),
    OverwriteErrToFile(Box<Expr>, String),
    AppendErrToFile(Box<Expr>, String),
    Pipe(Box<Expr>, Box<Expr>),
    Error, // Error is just here to be able to return something. I couldn't be bothered to write proper error handling (yet).
}
