use serde::Deserialize;
use std::{
    error::Error,
    fmt,
    fs::{create_dir_all, remove_dir, remove_file, File},
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    str::from_utf8,
};

static PARSER: &str = include_str!("../parser/dist/index.js");

#[derive(Debug, Deserialize, PartialEq)]
pub struct ParsedDoc {
    pub title: String,
    pub sections: Vec<Section>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Section {
    pub section_id: String,
    pub name: String,
    pub content: String,
    pub parents: Vec<String>,
}

#[derive(Debug)]
pub enum ParserError {
    Create(FailedToCreateParser),
    Delete(FailedToDeleteParser),
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

impl From<FailedToDeleteParser> for ParserError {
    fn from(err: FailedToDeleteParser) -> Self {
        Self::Delete(err)
    }
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParserError::Create(err) => write!(f, "Failed to create parser {:?}", err),
            ParserError::Delete(err) => write!(f, "Failed to delete parser {:?}", err),
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
            ParserError::Delete(err) => Some(err),
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

#[derive(Debug)]
pub struct FailedToDeleteParser(std::io::Error);

impl fmt::Display for FailedToDeleteParser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to delete parser file: {:?}", self.0)
    }
}

impl Error for FailedToDeleteParser {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

fn parser() -> Result<PathBuf, FailedToCreateParser> {
    let mut tmp = std::env::temp_dir();
    tmp.push(uuid::Uuid::new_v4().to_string());

    create_dir_all(&tmp).map_err(FailedToCreateParser)?;

    tmp.push("cio-rfd-parser");
    tmp.set_extension("js");

    let mut file = File::create(tmp.clone()).map_err(FailedToCreateParser)?;
    file.write_all(PARSER.as_bytes()).map_err(FailedToCreateParser)?;

    Ok(tmp)
}

pub fn parse(content: &str) -> Result<ParsedDoc, ParserError> {
    let mut tmp = parser()?;
    let path_arg = format!("{}", tmp.display());

    let mut cmd = Command::new("node")
        .args([path_arg])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    cmd.stdin
        .as_mut()
        .unwrap() // We always assign stdin above. Does that ensure this is Some?
        .write_all(content.as_bytes())?;
    let output = cmd.wait_with_output()?.stdout;

    remove_file(&tmp).map_err(FailedToDeleteParser)?;
    tmp.pop();
    remove_dir(&tmp).map_err(FailedToDeleteParser)?;

    serde_json::from_str(from_utf8(&output).map_err(ParserError::InvalidResponse)?)
        .map_err(ParserError::UnexpectedResponse)
}

#[cfg(test)]
mod tests {
    use super::*;

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

=== The First Option

First in the list

=== The Second Option

Second in the list

==== Further Nested Details

This options contains further information

=== The Third Option

Third in the list"#,
        )
        .unwrap();

        let expected = ParsedDoc {
            title: "On Parsing Documents".to_string(),
            sections: vec![
                Section {
                    section_id: "_background".to_string(),
                    name: "Background".to_string(),
                    content: "A paragraph about background topics".to_string(),
                    parents: vec![],
                },
                Section {
                    section_id: "_possibilities".to_string(),
                    name: "Possibilities".to_string(),
                    content: "Nested sections describing possible options".to_string(),
                    parents: vec![],
                },
                Section {
                    section_id: "_the_fist_option".to_string(),
                    name: "The First Option".to_string(),
                    content: "First in the list".to_string(),
                    parents: vec!["Possibilities".to_string()],
                },
                Section {
                    section_id: "_the_second_option".to_string(),
                    name: "The Second Option".to_string(),
                    content: "Second in the list".to_string(),
                    parents: vec!["Possibilities".to_string()],
                },
                Section {
                    section_id: "_further_nested_details".to_string(),
                    name: "Further Nested Details".to_string(),
                    content: "This options contains further information".to_string(),
                    parents: vec!["The Second Option".to_string(), "Possibilities".to_string()],
                },
                Section {
                    section_id: "_the_third_option".to_string(),
                    name: "The Third Option".to_string(),
                    content: "Third in the list".to_string(),
                    parents: vec!["Possibilities".to_string()],
                },
            ],
        };

        assert_eq!(expected, value);
    }
}
