use crate::cmd::{BuiltinStreamSource, BuiltinStreamTarget};
use crate::{exit_shell, ShellState};
use faccess::PathExt;
use rustyline::history::History;
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
    History,
    Alias,
    Unalias,
}

impl FromStr for BuiltinType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "history" => Ok(BuiltinType::History),
            "exit" => Ok(BuiltinType::Exit),
            "echo" => Ok(BuiltinType::Echo),
            "type" => Ok(BuiltinType::Type),
            "pwd" => Ok(BuiltinType::Pwd),
            "cd" => Ok(BuiltinType::Cd),
            "alias" => Ok(BuiltinType::Alias),
            "unalias" => Ok(BuiltinType::Unalias),
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

    pub(crate) fn set_args(&mut self, args: &mut Vec<String>) {
        self.args.append(args);
    }

    pub(crate) fn execute(&mut self, state: &mut ShellState) -> Result<(), Error> {
        match self.typ {
            BuiltinType::Exit => {
                exit_shell(state);
            }
            BuiltinType::Echo => {
                let mut out = self.args.join(" ");
                out.push('\n');
                self.write_stdout(out.as_str())?;
            }
            BuiltinType::Pwd => {
                if let Ok(current) = std::env::current_dir() {
                    self.write_stdout(&format!("{}\n", current.display()))?;
                } else {
                    self.write_stderr("Current working directory either doesn't exist or you have insufficient privileges\n")?;
                };
            }
            BuiltinType::Cd => {
                if let Some(next) = self.args.first() {
                    let target_path = &Self::create_path(next);
                    if std::env::set_current_dir(target_path).is_err() {
                        let message =
                            format!("cd: {}: No such file or directory\n", target_path.display());
                        self.write_stderr(&message)?;
                    }
                } else {
                    self.write_stderr("Cd requires at least one argument. If more than one are provided all but the first are discarded.\n")?;
                }
            }
            BuiltinType::Type => {
                if let Some(next) = self.args.first() {
                    if let Some(val) = state.aliases.get(next) {
                        self.write_stdout(&format!("{} is a alias for {}\n", next, val))?;
                    } else if BuiltinType::from_str(next).is_ok() {
                        let message = format!("{} is a shell builtin\n", next);
                        self.write_stdout(&message)?;
                    } else if let Some(path) = Self::search_for_executable(next) {
                        self.write_stdout(&format!("{}\n", path.display()))?;
                    } else {
                        let message = format!("{}: not found\n", next);
                        self.write_stdout(&message)?;
                    }
                } else {
                    self.write_stderr("Type requires at least one argument. If more than one are provided all but the first are discarded.\n")?;
                }
            }
            BuiltinType::History => {
                let see = self.args.first().map_or(state.rl.history().len(), |c| {
                    c.parse().unwrap_or(state.rl.history().len())
                });
                let history = state
                    .rl
                    .history()
                    .iter()
                    .skip(state.rl.history().len().saturating_sub(see));
                for (i, entry) in history.enumerate() {
                    self.write_stdout(&format!("{:>5}  {}\n", i + 1, entry))?;
                }
            }
            BuiltinType::Alias => {
                if let (Some(alias), Some(replacement)) = (self.args.first(), self.args.get(1)) {
                    if alias.contains('=') {
                        self.write_stderr("Alias can't contain '='.")?;
                    } else {
                        state.aliases.insert(alias.clone(), replacement.clone());
                        if let Some(helper) = state.rl.helper_mut() {
                            helper.get_commands_mut().insert(alias.clone());
                        }
                    }
                } else {
                    self.write_stderr("Alias requires at least two arguments.\n")?;
                }
            }
            BuiltinType::Unalias => {
                if let Some(alias) = self.args.first() {
                    state.aliases.remove(alias);
                    if let Some(helper) = state.rl.helper_mut() {
                        helper.get_commands_mut().remove(alias);
                    }
                } else {
                    self.write_stderr("Unalias requires one argument.\n")?;
                }
            }
        }
        Ok(())
    }

    fn write_stdout(&mut self, string: &str) -> Result<(), Error> {
        Self::write_out(&mut self.stdout_target, string)
    }

    fn write_stderr(&mut self, string: &str) -> Result<(), Error> {
        Self::write_out(&mut self.stderr_target, string)
    }

    fn write_out(target: &mut BuiltinStreamTarget, string: &str) -> Result<(), Error> {
        match target {
            BuiltinStreamTarget::InheritStdout => std::io::stdout().write_all(string.as_bytes())?,
            BuiltinStreamTarget::InheritStderr => std::io::stderr().write_all(string.as_bytes())?,
            BuiltinStreamTarget::BuiltinPipe(target) => {
                target.borrow_mut().replace_range(.., string)
            }
            BuiltinStreamTarget::Null => {}
            BuiltinStreamTarget::Pipe(target) => target.write_all(string.as_bytes())?,
            BuiltinStreamTarget::File(file) => {
                file.write_all(string.as_bytes())?;
            }
        }
        Ok(())
    }

    fn search_for_executable(name: &str) -> Option<PathBuf> {
        let path_var = std::env::var("PATH").unwrap_or_default();

        for dir_path in std::env::split_paths(&path_var) {
            let path = dir_path.join(name);
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
