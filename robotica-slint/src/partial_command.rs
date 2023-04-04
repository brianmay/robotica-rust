use std::ffi::OsString;

use thiserror::Error;

use crate::command::Line;

#[derive(Error, Debug)]
pub enum Error {
    #[error("No command given")]
    NoCommandGiven,
}

#[derive(Debug, Clone)]
pub struct PartialLine {
    command: OsString,
    args: Vec<OsString>,
}

impl PartialLine {
    pub fn new(args: Vec<String>) -> Result<Self, Error> {
        let mut iter = args.into_iter().map(OsString::from);
        let command = iter.next().ok_or(Error::NoCommandGiven)?;
        let args = iter.collect();
        Ok(Self { command, args })
    }

    pub fn to_line(&self) -> Line {
        Line::new(self.command.clone(), self.args.clone())
    }

    pub fn to_line_with_arg(&self, arg: impl Into<OsString>) -> Line {
        let mut args = self.args.clone();
        args.push(arg.into());
        Line::new(self.command.clone(), args)
    }

    pub fn to_line_with_args(&self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Line {
        let mut new_args = self.args.clone();
        new_args.extend(args.into_iter().map(std::convert::Into::into));
        Line::new(self.command.clone(), new_args)
    }
}
