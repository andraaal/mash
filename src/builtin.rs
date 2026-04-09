use crate::cmd::{BuiltinStreamSource, BuiltinStreamTarget};
use faccess::PathExt;
use std::io::Error;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

pub(crate) enum BuiltinType {
    Exit,
    Echo,
    Type,
    Pwd,
    Cd,
}

impl FromStr for BuiltinType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "exit" => Ok(BuiltinType::Exit),
            "echo" => Ok(BuiltinType::Echo),
            "type" => Ok(BuiltinType::Type),
            "pwd" => Ok(BuiltinType::Pwd),
            "cd" => Ok(BuiltinType::Cd),
            _ => Err(()),
        }
    }
}

pub(crate) struct Builtin {
    typ: BuiltinType,
    args: Vec<String>,
    stdin_target: BuiltinStreamSource,
    stdout_target: BuiltinStreamTarget,
    stderr_target: BuiltinStreamTarget,
}

impl Builtin {
    pub(crate) fn new(typ: &str) -> Result<Self, ()> {
        let builtin_typ: BuiltinType = typ.parse()?;
        Ok(Builtin {
            typ: builtin_typ,
            args: Vec::new(),
            stdin_target: BuiltinStreamSource::Inherit,
            stdout_target: BuiltinStreamTarget::InheritStdout,
            stderr_target: BuiltinStreamTarget::InheritStderr,
        })
    }

    pub(crate) fn set_stdout(&mut self, target: BuiltinStreamTarget) {
        self.stdout_target = target;
    }

    pub(crate) fn set_stdin(&mut self, target: BuiltinStreamSource) {
        self.stdin_target = target;
    }

    pub(crate) fn set_stderr(&mut self, target: BuiltinStreamTarget) {
        self.stderr_target = target;
    }

    pub(crate) fn set_args(&mut self, args: Vec<String>) {
        self.args = args;
    }

    pub(crate) fn execute(&mut self) -> Result<(), Error> {
        match self.typ {
            BuiltinType::Exit => std::process::exit(0),
            BuiltinType::Echo => {
                self.write_stdout(self.args.join(" ").as_str())?;
            }
            BuiltinType::Pwd => {
                if let Ok(current) = std::env::current_dir() {
                    self.write_stdout(current.display().to_string().as_str())?;
                } else {
                    self.write_stderr("Current working directory either doesn't exist or you have insufficient privileges")?;
                };
            }
            BuiltinType::Cd => {
                if let Some(next) = self.args.get(0) {
                    let target_path = &Self::create_path(next);
                    if std::env::set_current_dir(target_path).is_err() {
                        let message = format!("cd: {}: No such file or directory", target_path.display());
                        self.write_stderr(&message)?;
                    }
                } else {
                    self.write_stderr("Cd requires at least one argument. If more than one are provided all but the first are discarded.")?;
                }
            }
            BuiltinType::Type => {
                if let Some(next) = self.args.get(0) {
                    if BuiltinType::from_str(next).is_ok() {
                        let message = format!("{} is a shell builtin", next);
                        self.write_stdout(&message)?;
                    } else if let Some(path) = Self::search_for_executable(next) {
                        self.write_stdout(path.display().to_string().as_str())?;
                    } else {
                        let message = format!("{}: not found", next);
                        self.write_stdout(&message)?;
                    }
                } else {
                    self.write_stderr("Type requires at least one argument. If more than one are provided all but the first are discarded.")?;
                }
            }
        }
        Ok(())
    }

    fn write_stdout(&mut self, string: &str) -> Result<(), Error> {
        match self.stdout_target {
            BuiltinStreamTarget::InheritStdout => std::io::stdout().write_all(string.as_bytes())?,
            BuiltinStreamTarget::InheritStderr => std::io::stderr().write_all(string.as_bytes())?,
            BuiltinStreamTarget::BuiltinPipe(_) => todo!("Builtin to builtin piping is not supported yet"),
            BuiltinStreamTarget::Null => {}
            BuiltinStreamTarget::Pipe(ref mut target) => target.write_all(string.as_bytes())?,
        }
        Ok(())
    }

    fn write_stderr(&mut self, string: &str) -> Result<(), Error> {
        match self.stderr_target {
            BuiltinStreamTarget::InheritStdout => std::io::stdout().write_all(string.as_bytes())?,
            BuiltinStreamTarget::InheritStderr => std::io::stderr().write_all(string.as_bytes())?,
            BuiltinStreamTarget::BuiltinPipe(_) => todo!("Builtin to builtin piping is not supported yet"),
            BuiltinStreamTarget::Null => {}
            BuiltinStreamTarget::Pipe(ref mut target) => target.write_all(string.as_bytes())?,
        }
        Ok(())
    }

    fn search_for_executable(name: &str) -> Option<PathBuf> {
        let path_var = std::env::var("PATH").unwrap();

        for path_str in path_var.split(":") {
            let path = PathBuf::new().join(format!("{}/{}", path_str, name).as_str());
            if path.executable() {
                return Some(path);
            }
        }
        None
    }

    fn create_path(string: &str) -> PathBuf {
        let mut path = string.to_string();
        #[cfg(target_family = "unix")]
        {
            let home = std::env::var("HOME").unwrap_or_default();
            path = path.replace("~", home.as_str());
        }
        PathBuf::from(path)
    }
}
