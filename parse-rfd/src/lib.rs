use serde::Deserialize;
use std::{
    io::Write,
    process::{Command, Stdio}
};

#[derive(Deserialize)]
pub struct SearchAttributes {
    pub section_id: String,
    pub anchor: String,
    pub name: String,
    pub level: u32,
    pub content: String,
    pub hierarchy_lvl0: Option<String>,
    pub hierarchy_lvl1: Option<String>,
    pub hierarchy_lvl2: Option<String>,
    pub hierarchy_lvl3: Option<String>,
    pub hierarchy_lvl4: Option<String>,
    pub hierarchy_lvl5: Option<String>,
    pub hierarchy_lvl6: Option<String>,
    pub hierarchy_radio_lvl0: Option<String>,
    pub hierarchy_radio_lvl1: Option<String>,
    pub hierarchy_radio_lvl2: Option<String>,
    pub hierarchy_radio_lvl3: Option<String>,
    pub hierarchy_radio_lvl4: Option<String>,
    pub hierarchy_radio_lvl5: Option<String>,
}

pub fn parse(content: &str) -> SearchAttributes {
    let mut cmd = Command::new("node")
        .args(["parser/index.js"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    cmd
        .stdin
        .as_mut()
        .unwrap()
        .write_all(content.as_bytes());

    serde_json::from_str(std::str::from_utf8(&cmd.wait_with_output().unwrap().stdout).unwrap()).unwrap()
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        crate::parse("from test");
    }
}
