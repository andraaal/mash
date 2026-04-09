use crate::args::{Args, Token};
use crate::builtin::Builtin;
use std::cell::RefCell;
use std::io::{pipe, Error, PipeReader, PipeWriter};
use std::iter::Peekable;
use std::process::{Command, Stdio};
use std::rc::Rc;

pub(crate) struct Parser<'a> {
    tokens: Peekable<Args<'a>>,
    expressions: Vec<Expr>,
    errors: Vec<String>,
}

impl<'a> Parser<'_> {
    pub fn new(args: Peekable<Args<'a>>) -> Parser {
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
            // TODO: Check for expression separator (\n) here
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

    // TODO: implement precedence (and associativity)
    fn parse_precedence(&mut self, min_prec: u32) -> Expr {
        if let Some(prefix_tk) = self.next_token() {
            let mut lhs;
            if let Some(parselet) = Self::prefix_parselet(&prefix_tk) {
                lhs = (parselet.parse)(self, prefix_tk);
            } else {
                self.errors.push(format!("Invalid start of expression: {:?}", prefix_tk));
                return Expr::Error;
            }

            while let Some(infix_tk) = self.peek_token() {
                if let Some(parselet) = Self::infix_parselet(&infix_tk)
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
            self.errors.push("Invalid start of expression: EOF".to_string());
            Expr::Error
        }
    }

    fn next_token(&mut self) -> Option<Token> {
        self.tokens.next()
    }
    fn peek_token(&mut self) -> Option<&Token> {
        self.tokens.peek()
    }

    fn consume(&mut self, condition: fn(&Token) -> bool) -> bool {
        if self.tokens.peek().is_some_and(|tk| condition(tk)) {
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

    // Any token that can't be at the start of an expression is considered infix
    const fn infix_parselet(tk: &Token) -> Option<InfixParselet> {
        let tp = match tk {
            Token::RedirectOutToFile => InfixParselet {
                precedence: 10,
                parse: |parser, _token, lhs| {
                    let rhs = parser.consume_symbol();
                    if let Some(right) = rhs {
                        Expr::RedirectOut(Box::new(lhs), right)
                    } else {
                        parser
                            .errors
                            .push("Expected filename after redirect".to_string());
                        Expr::Error
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
                    let mut command = Cmd::new(&token.to_text());
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
    RedirectOut(Box<Expr>, String),
    Pipe(Box<Expr>, Box<Expr>),
    Error, // Error is just here to be able to return something. I couldn't be bothered to write proper error handling (yet).
}

// Define the target of the streams here; then start the process to convert into a Cmd
pub(crate) enum Cmd {
    External(Command),
    Builtin(Builtin),
}

pub(crate) enum BuiltinStreamTarget {
    // Internal Stream Target
    InheritStdout,                    // Piped to the Stdout of the parent process
    InheritStderr,                    // Piped to the Stderr of the parent process
    BuiltinPipe(Rc<RefCell<String>>), // Doesn't need to be implemented yet: simply create todo!
    Null,                             // To the void
    Pipe(PipeWriter),                 // Piped to the Stdin of the child
}

pub(crate) enum BuiltinStreamSource {
    // Internal Stream Source
    Inherit,                          // Piped from the Stdin of the parent process
    BuiltinPipe(Rc<RefCell<String>>), // Doesn't need to be implemented yet: simply create todo!
    Null,                             // From the void
    Pipe(PipeReader),                 // Get input from this pipe
}

pub(crate) enum StreamTarget<'a> {
    InheritStdout,
    InheritStderr,
    Null,
    Child(&'a mut Cmd),
}
pub(crate) enum StreamSource<'a> {
    Inherit,
    Null,
    ChildStdout(&'a mut Cmd),
    ChildStderr(&'a mut Cmd),
}

impl Cmd {
    pub(crate) fn new(name: &str) -> Self {
        if let Ok(builtin) = Builtin::new(name) {
            Cmd::Builtin(builtin)
        } else {
            let mut cmd = Command::new(name);
            cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
            Cmd::External(Command::new(name))
        }
    }

    pub(crate) fn set_stdin(&mut self, target: StreamSource) -> Result<(), Error> {
        match self {
            Cmd::External(command) => {
                let stdio: Stdio = match target {
                    StreamSource::Inherit => Stdio::inherit(),
                    StreamSource::Null => Stdio::null(),
                    StreamSource::ChildStdout(child) => {
                        let (reader, writer): (PipeReader, PipeWriter) = pipe()?;
                        match child {
                            Cmd::External(external) => {
                                external.stdout(writer);
                            }
                            Cmd::Builtin(builtin) => {
                                builtin.set_stdout(BuiltinStreamTarget::Pipe(writer));
                            }
                        }
                        reader.into()
                    }
                    StreamSource::ChildStderr(child) => {
                        let (reader, writer): (PipeReader, PipeWriter) = pipe()?;
                        match child {
                            Cmd::External(external) => {
                                external.stderr(writer);
                            }
                            Cmd::Builtin(builtin) => {
                                builtin.set_stderr(BuiltinStreamTarget::Pipe(writer));
                            }
                        }
                        reader.into()
                    }
                };
                command.stdin(stdio);
            }
            Cmd::Builtin(builtin) => {
                let source = match target {
                    StreamSource::Inherit => BuiltinStreamSource::Inherit,
                    StreamSource::Null => BuiltinStreamSource::Null,
                    StreamSource::ChildStdout(child) => match child {
                        Cmd::External(external) => {
                            let (reader, writer): (PipeReader, PipeWriter) = pipe()?;
                            external.stdout(writer);
                            BuiltinStreamSource::Pipe(reader)
                        }
                        Cmd::Builtin(_builtin) => {
                            todo!("Piping from builtin to builtin not supported yet")
                        }
                    },
                    StreamSource::ChildStderr(child) => match child {
                        Cmd::External(external) => {
                            let (reader, writer): (PipeReader, PipeWriter) = pipe()?;
                            external.stderr(writer);
                            BuiltinStreamSource::Pipe(reader)
                        }
                        Cmd::Builtin(_builtin) => {
                            todo!("Piping from builtin to builtin not supported yet")
                        }
                    },
                };
                builtin.set_stdin(source);
            }
        }
        Ok(())
    }

    pub(crate) fn set_stdout(&mut self, target: StreamTarget) -> Result<(), Error> {
        match self {
            Cmd::External(command) => {
                let stdio = match target {
                    StreamTarget::InheritStdout => Stdio::inherit(),
                    StreamTarget::InheritStderr => std::io::stderr().into(),
                    StreamTarget::Null => Stdio::null(),
                    StreamTarget::Child(child) => {
                        let (reader, writer): (PipeReader, PipeWriter) = pipe()?;
                        match child {
                            Cmd::External(external) => {
                                external.stdin(reader);
                            }
                            Cmd::Builtin(builtin) => {
                                builtin.set_stdin(BuiltinStreamSource::Pipe(reader));
                            }
                        }
                        writer.into()
                    }
                };
                command.stdout(stdio);
            }
            Cmd::Builtin(builtin) => {
                let mapped = match target {
                    StreamTarget::InheritStdout => BuiltinStreamTarget::InheritStdout,
                    StreamTarget::InheritStderr => BuiltinStreamTarget::InheritStderr,
                    StreamTarget::Null => BuiltinStreamTarget::Null,
                    StreamTarget::Child(child) => match child {
                        Cmd::External(external) => {
                            let (reader, writer): (PipeReader, PipeWriter) = pipe()?;
                            external.stdin(reader);
                            BuiltinStreamTarget::Pipe(writer)
                        }
                        Cmd::Builtin(_builtin) => {
                            todo!("Piping from builtin to builtin not supported yet")
                        }
                    },
                };
                builtin.set_stdout(mapped);
            }
        }
        Ok(())
    }

    pub(crate) fn set_stderr(&mut self, target: StreamTarget) -> Result<(), Error> {
        match self {
            Cmd::External(command) => {
                let stdio: Stdio = match target {
                    StreamTarget::InheritStdout => std::io::stdout().into(),
                    StreamTarget::InheritStderr => Stdio::inherit(),
                    StreamTarget::Null => Stdio::null(),
                    StreamTarget::Child(child) => {
                        let (reader, writer): (PipeReader, PipeWriter) = pipe()?;
                        match child {
                            Cmd::External(external) => {
                                external.stdin(reader);
                            }
                            Cmd::Builtin(builtin) => {
                                builtin.set_stdin(BuiltinStreamSource::Pipe(reader));
                            }
                        }
                        writer.into()
                    }
                };
                command.stderr(stdio);
            }
            Cmd::Builtin(builtin) => {
                let mapped = match target {
                    StreamTarget::InheritStdout => BuiltinStreamTarget::InheritStdout,
                    StreamTarget::InheritStderr => BuiltinStreamTarget::InheritStderr,
                    StreamTarget::Null => BuiltinStreamTarget::Null,
                    StreamTarget::Child(child) => match child {
                        Cmd::External(external) => {
                            let (reader, writer): (PipeReader, PipeWriter) = pipe()?;
                            external.stdin(reader);
                            BuiltinStreamTarget::Pipe(writer)
                        }
                        Cmd::Builtin(_builtin) => {
                            todo!("Piping from builtin to builtin not supported yet")
                        }
                    },
                };
                builtin.set_stderr(mapped);
            }
        }
        Ok(())
    }

    pub(crate) fn wait(&mut self) -> Result<(), Error> {
        match self {
            Cmd::External(command) => {
                command.output()?;
            }
            Cmd::Builtin(builtin) => {
                builtin.execute()?;
            }
        };
        Ok(())
    }

    pub(crate) fn set_args(&mut self, args: Vec<String>) {
        match self {
            Cmd::External(command) => {
                command.args(args);
            }
            Cmd::Builtin(builtin) => {
                builtin.set_args(args);
            }
        }
    }
}
