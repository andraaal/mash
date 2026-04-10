use std::cell::RefCell;
use std::fs::File;
use std::io::{pipe, Error, PipeReader, PipeWriter};
use std::process::{Command, Stdio};
use std::rc::Rc;
use crate::builtin::Builtin;

// Define the target of the streams here; then start the process to convert into a Cmd
pub(crate) enum Cmd {
    External(Command),
    Builtin(Builtin),
}

pub(crate) enum BuiltinStreamTarget {
    // Internal Stream Target
    InheritStdout,                    // Piped to the Stdout of the parent process
    InheritStderr,                    // Piped to the Stderr of the parent process
    BuiltinPipe(Rc<RefCell<String>>), // Just written to the shared string
    Null,                             // To the void
    Pipe(PipeWriter),                 // Piped to the Stdin of the child
    File(File),
}

pub(crate) enum BuiltinStreamSource {
    // Internal Stream Source
    Inherit,                          // Piped from the Stdin of the parent process
    BuiltinPipe(Rc<RefCell<String>>), // Just read from the shared string
    Null,                             // From the void
    Pipe(PipeReader),                 // Get input from this pipe
    File(File),
}

pub(crate) enum StreamTarget<'a> {
    InheritStdout,
    InheritStderr,
    Null,
    Child(&'a mut Cmd),
    File(File),
}
pub(crate) enum StreamSource<'a> {
    Inherit,
    Null,
    ChildStdout(&'a mut Cmd),
    ChildStderr(&'a mut Cmd),
    File(File),
}

impl Cmd {
    pub(crate) fn new(name: &str) -> Self {
        if let Ok(builtin) = Builtin::new(name) {
            Cmd::Builtin(builtin)
        } else {
            let mut cmd = Command::new(name);
            cmd.stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
            Cmd::External(cmd)
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
                    StreamSource::File(file) => file.into(),
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
                    StreamSource::File(file) => BuiltinStreamSource::File(file),
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
                    StreamTarget::File(file) => file.into(),
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
                    StreamTarget::File(file) => BuiltinStreamTarget::File(file),
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
                    StreamTarget::File(file) => file.into(),
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
                    StreamTarget::File(file) => BuiltinStreamTarget::File(file),
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
