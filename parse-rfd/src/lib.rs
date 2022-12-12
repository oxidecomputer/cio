use serde::Deserialize;
use std::{
    error::Error,
    fmt,
    fs::File,
    io::Write,
    process::{Command, Stdio},
    str::from_utf8,
};

static PARSER: &str = include_str!("../parser/dist/index.js");

#[derive(Debug, Deserialize)]
pub struct Section {
    pub section_id: String,
    pub name: String,
    pub content: String,
    pub parents: Vec<String>,
}

#[derive(Debug)]
pub enum ParserError {
    Create(FailedToCreateParser),
    Execute(std::io::Error),
    InvalidResponse(std::str::Utf8Error),
    UnexpectedResponse(serde_json::Error),
}

impl From<std::io::Error> for ParserError {
    fn from(err: std::io::Error) -> Self {
        Self::Execute(err)
    }
}

impl From<FailedToCreateParser> for ParserError {
    fn from(err: FailedToCreateParser) -> Self {
        Self::Create(err)
    }
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParserError::Create(err) => write!(f, "Failed to create parser {:?}", err),
            ParserError::Execute(err) => write!(f, "Failed to run parser {:?}", err),
            ParserError::InvalidResponse(err) => write!(f, "Parser return unusable data {:?}", err),
            ParserError::UnexpectedResponse(err) => write!(f, "Parser return data that could not be parsed {:?}", err),
        }
    }
}

impl Error for ParserError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ParserError::Create(err) => Some(err),
            ParserError::Execute(err) => Some(err),
            ParserError::InvalidResponse(err) => Some(err),
            ParserError::UnexpectedResponse(err) => Some(err),
        }
    }
}

#[derive(Debug)]
pub struct FailedToCreateParser(std::io::Error);

impl fmt::Display for FailedToCreateParser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to create parser file: {:?}", self.0)
    }
}

impl Error for FailedToCreateParser {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

fn parser() -> Result<String, FailedToCreateParser> {
    let mut tmp = std::env::temp_dir();
    tmp.push("cio-rfd-parser");
    tmp.set_extension("js");

    let path_arg = format!("{}", tmp.display());

    if !tmp.exists() {
        let mut file = File::create(tmp).map_err(FailedToCreateParser)?;
        file.write_all(PARSER.as_bytes()).map_err(FailedToCreateParser)?;
    }

    Ok(path_arg)
}

pub fn parse(content: &str) -> Result<Vec<Section>, ParserError> {
    let mut cmd = Command::new("node")
        .args([parser()?])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    cmd.stdin
        .as_mut()
        .unwrap() // We always assign stdin above. Does that ensure this is Some?
        .write_all(content.as_bytes())?;
    let output = cmd.wait_with_output()?.stdout;

    serde_json::from_str(from_utf8(&output).map_err(ParserError::InvalidResponse)?)
        .map_err(ParserError::UnexpectedResponse)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_sections() {
        let value = crate::parse(
            r#":showtitle:
:toc: left
:numbered:
:icons: font
:state: published
:discussion: https://github.com/organization/repo/pull/123
:revremark: State: {state} | {discussion}
:authors: Firstname Lastname <author@organization.com>

= RFD 123 On Parsing Documents
{authors}

An introductory line about the document

== Background

A paragraph about background topics

== Possibilities

Nested sections describing possible options

=== The Fist Option

First in the list

=== The Second Option

Second in the list

==== Further Nested Details

This options contains further information

=== The Third Option

Third in the list"#,
        );

        panic!("{:#?}", value);
    }
}
