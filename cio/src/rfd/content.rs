use anyhow::{bail, Result};
use comrak::{markdown_to_html, ComrakOptions};
use log::info;
use regex::Regex;
use std::{
    borrow::Cow,
    env, fmt, fs,
    path::{Path, PathBuf},
    process::Command,
    str::from_utf8,
};
use uuid::Uuid;

use super::{GitHubRFDBranch, RFDNumber, RFDPdf};
use crate::utils::{decode_base64, write_file};

// TODO: RFDNumber should probably be stored with the content as it doesn't parsing content with a
// mismatched RFDNumber is pretty nonsensical.

#[derive(Debug)]
pub enum RFDContent<'a> {
    Asciidoc(RFDAsciidoc<'a>),
    Markdown(RFDMarkdown<'a>),
}

impl<'a> RFDContent<'a> {
    /// Create a new RFDContent wrapper and attempt to determine the type by examining the source
    /// contents.
    pub fn new<T>(content: T) -> Result<Self>
    where
        T: Into<Cow<'a, str>>,
    {
        // TODO: This content inspection should be replaced by actually storing the format of the
        // content in the RFD struct
        let content = content.into();

        let is_asciidoc = {
            // A regular expression that looks for commonly used Asciidoc attributes. These are static
            // regular expressions and can be safely unwrapped.
            let attribute_check = Regex::new(r"(?m)^(:showtitle:|:numbered:|:toc: left|:icons: font)$").unwrap();
            let state_check =
                Regex::new(r"(?m)^:state: (ideation|prediscussion|discussion|abandoned|published|committed) *?$")
                    .unwrap();

            // Check that the content contains at least one of the commonly used asciidoc attributes,
            // and contains a state line.
            attribute_check.is_match(&content) && state_check.is_match(&content)
        };

        let is_markdown = !is_asciidoc && {
            let title_check = Regex::new(r"(?m)^# RFD").unwrap();
            let state_check =
                Regex::new(r"(?m)^state: (ideation|prediscussion|discussion|abandoned|published|committed) *?$")
                    .unwrap();

            title_check.is_match(&content) && state_check.is_match(&content)
        };

        // Return the content wrapped in the appropriate format wrapper
        if is_asciidoc {
            Ok(Self::new_asciidoc(content))
        } else if is_markdown {
            Ok(Self::new_markdown(content))
        } else {
            // If neither content type can be detected than we return an error
            bail!("Failed to detect if the content was either Asciidoc or Markdown")
        }
    }

    /// Construct a new RFDContent wrapper that contains Asciidoc content
    pub fn new_asciidoc<T>(content: T) -> Self
    where
        T: Into<Cow<'a, str>>,
    {
        Self::Asciidoc(RFDAsciidoc::new(content.into()))
    }

    /// Construct a new RFDContent wrapper that contains Markdown content
    pub fn new_markdown<T>(content: T) -> Self
    where
        T: Into<Cow<'a, str>>,
    {
        Self::Markdown(RFDMarkdown::new(content.into()))
    }

    /// Get a reference to the internal unparsed contents
    pub fn raw(&self) -> &str {
        match self {
            Self::Asciidoc(adoc) => &adoc.content,
            Self::Markdown(md) => &md.content,
        }
    }

    /// Consume this wrapper and return the internal unparsed contents
    pub fn into_inner(self) -> String {
        match self {
            Self::Asciidoc(adoc) => adoc.content.into_owned(),
            Self::Markdown(md) => md.content.into_owned(),
        }
    }

    /// Generate an HTML string by combining RFD contents with static resources that are stored for
    /// a given RFD number on a specific branch
    pub async fn to_html(&self, number: &RFDNumber, branch: &GitHubRFDBranch) -> Result<RFDHtml> {
        match self {
            Self::Asciidoc(adoc) => adoc.to_html(number, branch).await,
            Self::Markdown(md) => md.to_html(number),
        }
    }

    /// Generate a PDF by combining RFD contents with static resources that are stored for a given
    /// RFD number on a specific branch. Markdown documents do not support PDF generation
    pub async fn to_pdf(
        &self,
        title: &str,
        number: &RFDNumber,
        branch: &GitHubRFDBranch,
    ) -> Result<RFDPdf, RFDOutputError> {
        match self {
            Self::Asciidoc(adoc) => adoc
                .to_pdf(title, number, branch)
                .await
                .map_err(RFDOutputError::Generic),
            _ => Err(RFDOutputError::FormatNotSupported(RFDOutputFormat::Pdf)),
        }
    }

    /// Update the discussion link stored within the document to the passed link
    pub fn update_discussion_link(&mut self, link: &str) {
        let (re, pre, content) = match self {
            RFDContent::Asciidoc(ref mut adoc) => (
                Regex::new(r"(?m)(:discussion:.*$)").unwrap(),
                ":",
                adoc.content.to_mut(),
            ),
            RFDContent::Markdown(ref mut md) => (Regex::new(r"(?m)(discussion:.*$)").unwrap(), "", md.content.to_mut()),
        };

        let replacement = if let Some(v) = re.find(content) {
            v.as_str().to_string()
        } else {
            String::new()
        };

        *content = content.replacen(&replacement, &format!("{}discussion: {}", pre, link.trim()), 1);
    }

    /// Update the state stored within the document to the passed state
    pub fn update_state(&mut self, state: &str) {
        let (re, pre, content) = match self {
            RFDContent::Asciidoc(ref mut adoc) => {
                (Regex::new(r"(?m)(:state:.*$)").unwrap(), ":", adoc.content.to_mut())
            }
            RFDContent::Markdown(ref mut md) => (Regex::new(r"(?m)(state:.*$)").unwrap(), "", md.content.to_mut()),
        };

        let replacement = if let Some(v) = re.find(content) {
            v.as_str().to_string()
        } else {
            String::new()
        };

        *content = content.replacen(&replacement, &format!("{}state: {}", pre, state.trim()), 1);
    }

    /// Extract the title from the internal content
    pub fn get_title(&self) -> String {
        let content = self.raw();

        let mut re = Regex::new(r"(?m)(RFD .*$)").unwrap();
        match re.find(content) {
            Some(v) => {
                // TODO: find less horrible way to do this.
                let trimmed = v
                    .as_str()
                    .replace("RFD", "")
                    .replace("# ", "")
                    .replace("= ", " ")
                    .trim()
                    .to_string();

                let (_, s) = trimmed.split_once(' ').unwrap();
                s.to_string()
            }
            None => {
                // There is no "RFD" in our title. This is the case for RFD 31.
                re = Regex::new(r"(?m)(^= .*$)").unwrap();
                let c = re.find(content);

                if let Some(results) = c {
                    results
                        .as_str()
                        .replace("RFD", "")
                        .replace("# ", "")
                        .replace("= ", " ")
                        .trim()
                        .to_string()
                } else {
                    // If we couldn't find anything assume we have no title.
                    // This was related to this error in Sentry:
                    // https://sentry.io/organizations/oxide-computer-company/issues/2701636092/?project=-1
                    String::new()
                }
            }
        }
    }

    /// Get the state value stored within the document. If one can not be found, then an empty
    /// string is returned
    pub fn get_state(&self) -> String {
        let re = Regex::new(r"(?m)(state:.*$)").unwrap();

        // TODO: This should return Option<&str>
        match re.find(self.raw()) {
            Some(v) => v.as_str().replace("state:", "").trim().to_string(),
            None => Default::default(),
        }
    }

    /// Get the discussion link stored within the document. If one can not be found, then an empty
    /// string is returned
    pub fn get_discussion(&self) -> String {
        let re = Regex::new(r"(?m)(discussion:.*$)").unwrap();

        // TODO: This should return Option<&str>
        match re.find(self.raw()) {
            Some(v) => {
                let d = v.as_str().replace("discussion:", "").trim().to_string();

                if !d.starts_with("http") {
                    Default::default()
                } else {
                    d
                }
            }
            None => Default::default(),
        }
    }

    /// Get the authors line stored within the document. The returned string may contain multiple
    /// names. If none can be found, then and empty string is returned
    pub fn get_authors(&self) -> String {
        match self {
            Self::Asciidoc(RFDAsciidoc { content, .. }) => {
                // We must have asciidoc content.
                // We want to find the line under the first "=" line (which is the title), authors
                // is under that.
                let re = Regex::new(r"(?m:^=.*$)[\n\r](?m)(.*$)").unwrap();
                match re.find(content) {
                    Some(v) => {
                        let val = v.as_str().trim().to_string();
                        let parts: Vec<&str> = val.split('\n').collect();
                        if parts.len() < 2 {
                            Default::default()
                        } else {
                            let mut authors = parts[1].to_string();
                            if authors == "{authors}" {
                                // Do the traditional check.
                                let re = Regex::new(r"(?m)(^:authors.*$)").unwrap();
                                if let Some(v) = re.find(content) {
                                    authors = v.as_str().replace(":authors:", "").trim().to_string();
                                }
                            }
                            authors
                        }
                    }
                    None => Default::default(),
                }
            }
            Self::Markdown(RFDMarkdown { content }) => {
                // TODO: make work w asciidoc.
                let re = Regex::new(r"(?m)(^authors.*$)").unwrap();
                match re.find(content) {
                    Some(v) => v.as_str().replace("authors:", "").trim().to_string(),
                    None => Default::default(),
                }
            }
        }
    }
}

/// The text data of an Asciidoc RFD
#[derive(Debug)]
pub struct RFDAsciidoc<'a> {
    content: Cow<'a, str>,
    storage_id: Uuid,
}

impl<'a> RFDAsciidoc<'a> {
    pub fn new(content: Cow<'a, str>) -> Self {
        Self {
            content,
            storage_id: Uuid::new_v4(),
        }
    }

    /// Generate an HTML string by combining RFD contents with static resources that are stored for
    /// a given RFD number on a specific branch
    pub async fn to_html(&self, number: &RFDNumber, branch: &GitHubRFDBranch) -> Result<RFDHtml> {
        self.download_images(number, branch).await?;

        let mut html = RFDHtml(from_utf8(&self.parse(RFDOutputFormat::Html).await?)?.to_string());
        html.clean_links(&number.as_number_string());

        Ok(html)
    }

    /// Generate a PDF by combining RFD contents with static resources that are stored for a given
    /// RFD number on a specific branch. Markdown documents do not support PDF generation
    pub async fn to_pdf(&self, title: &str, number: &RFDNumber, branch: &GitHubRFDBranch) -> Result<RFDPdf> {
        self.download_images(number, branch).await?;

        let content = self.parse(RFDOutputFormat::Pdf).await?;

        let filename = format!(
            "RFD {} {}.pdf",
            number.as_number_string(),
            title.replace('/', "-").replace('\'', "").replace(':', "").trim()
        );

        Ok(RFDPdf {
            filename,
            contents: content,
            number: *number,
        })
    }

    /// Parse the asciidoc content and generate output data of the requested format. This relies on
    /// invoking an external asciidoctor binary to perform the actual transformation.
    async fn parse(&self, format: RFDOutputFormat) -> Result<Vec<u8>> {
        info!("[asciidoc] Parsing asciidoc file");

        // Create the path to the local tmp file for holding the asciidoc contents
        let storage_path = self.tmp_path();
        let file_path = storage_path.join("contents.adoc");

        // // Write the contents to a temporary file.
        write_file(&file_path, self.content.as_bytes()).await?;

        info!("[asciidoc] Wrote file to temp dir {:?}", file_path);

        let cmd_output = tokio::task::spawn_blocking(enclose! { (storage_path, file_path) move || {
            info!("[asciidoc] Shelling out to asciidoctor {:?} / {:?}", storage_path, file_path);
            let out = format.command(&storage_path, &file_path).output();

            match &out {
                Ok(_) => info!("[asciidoc] Command succeeded {:?} / {:?}", storage_path, file_path),
                Err(err) => info!("[asciidoc] Command failed: {} {:?} / {:?}", err, storage_path, file_path)
            };

            out
        }})
        .await??;

        info!("[asciidoc] Completed asciidoc rendering");

        let result = if cmd_output.status.success() {
            cmd_output.stdout
        } else {
            bail!(
                "[rfds] running asciidoctor failed: {} {}",
                from_utf8(&cmd_output.stdout)?,
                from_utf8(&cmd_output.stderr)?
            );
        };

        if let Err(err) = self.cleanup_tmp_path() {
            log::error!("Failed to clean up temporary working files for {:?} {:?}", format, err);
        }

        info!("[asciidoc] Finished cleanup and returning");

        Ok(result)
    }

    /// Downloads images that are stored on the provided GitHub branch for the given RFD number.
    /// These are stored locally so in a tmp directory for use by asciidoctor
    async fn download_images(&self, number: &RFDNumber, branch: &GitHubRFDBranch) -> Result<()> {
        let dir = number.repo_directory();

        let storage_path = self.tmp_path();
        let storage_path_string = storage_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Unable to convert image temp storage path to string"))?;
        let images = branch.get_images(number).await?;

        for image in images {
            // Save the image to our temporary directory.
            let image_path = format!(
                "{}/{}",
                storage_path_string,
                image
                    .path
                    .replace(&dir.trim_start_matches('/'), "")
                    .trim_start_matches('/')
            );

            let path = PathBuf::from(image_path);

            write_file(&path, &decode_base64(&image.content)).await?;

            info!(
                "[asciidoc] Wrote embedded image to {:?} / {} / {}",
                path, number, branch.branch
            );
        }

        Ok(())
    }

    /// Computes the temporary path for use when generating asciidoc HTML. This returns None for
    /// markdown files as no temporary storage is used
    fn tmp_path(&self) -> PathBuf {
        let mut path = env::temp_dir();
        path.push("asciidoc-rfd-render/");
        path.push(&self.storage_id.to_string());

        path
    }

    // Cleanup remaining images and local state that was used by asciidoctor
    fn cleanup_tmp_path(&self) -> Result<()> {
        let storage_path = self.tmp_path();

        if storage_path.exists() && storage_path.is_dir() {
            log::info!("Removing temporary content directory {:?}", storage_path);

            fs::remove_dir_all(storage_path)?
        }

        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
pub enum RFDOutputFormat {
    Html,
    Pdf,
}

impl RFDOutputFormat {
    /// Generate a command for parsing asciidoctor content
    pub fn command(&self, working_dir: &PathBuf, file_path: &Path) -> Command {
        match self {
            Self::Html => {
                let mut command = Command::new("asciidoctor");
                command
                    .current_dir(working_dir)
                    .args(&["-o", "-", "--no-header-footer", file_path.to_str().unwrap()]);

                command
            }
            Self::Pdf => {
                let mut command = Command::new("asciidoctor-pdf");
                command.current_dir(working_dir).args(&[
                    "-o",
                    "-",
                    "-r",
                    "asciidoctor-mermaid/pdf",
                    "-a",
                    "source-highlighter=rouge",
                    file_path.to_str().unwrap(),
                ]);

                command
            }
        }
    }
}

#[derive(Debug)]
pub enum RFDOutputError {
    FormatNotSupported(RFDOutputFormat),
    Generic(anyhow::Error),
}

impl fmt::Display for RFDOutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FormatNotSupported(format) => write!(f, "{:?} format is not supported", format),
            Self::Generic(inner) => write!(f, "Failed to generate RFD output due to {:?}", inner),
        }
    }
}

impl std::error::Error for RFDOutputError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Generic(inner) => Some(inner.as_ref()),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct RFDMarkdown<'a> {
    content: Cow<'a, str>,
}

impl<'a> RFDMarkdown<'a> {
    pub fn new(content: Cow<'a, str>) -> Self {
        Self { content }
    }

    /// Generate an HTML string by combining RFD contents with static resources that are stored for
    /// a given RFD number on a specific branch
    pub fn to_html(&self, number: &RFDNumber) -> Result<RFDHtml> {
        let mut html = RFDHtml(markdown_to_html(&self.content, &ComrakOptions::default()));
        html.clean_links(&number.as_number_string());

        Ok(html)
    }
}

pub struct RFDHtml(pub String);

impl RFDHtml {
    /// Replaces link relative to the document with links relative to the root of the RFD repo.
    /// Also replaces urls of the form (<num>\d+).rfd.oxide.computer with urls that look like
    /// rfd.shared.oxide.computer/rfd/$num where $num is left padded with 0s
    pub fn clean_links(&mut self, num: &str) {
        let mut cleaned = self
            .0
            .replace(r#"href="\#"#, &format!(r#"href="/rfd/{}#"#, num))
            .replace("href=\"#", &format!("href=\"/rfd/{}#", num))
            .replace(r#"img src=""#, &format!(r#"img src="/static/images/{}/"#, num))
            .replace(r#"object data=""#, &format!(r#"object data="/static/images/{}/"#, num))
            .replace(
                r#"object type="image/svg+xml" data=""#,
                &format!(r#"object type="image/svg+xml" data="/static/images/{}/"#, num),
            );

        let mut re = Regex::new(r"https://(?P<num>[0-9]).rfd.oxide.computer").unwrap();
        cleaned = re
            .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/000$num")
            .to_string();
        re = Regex::new(r"https://(?P<num>[0-9][0-9]).rfd.oxide.computer").unwrap();
        cleaned = re
            .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/00$num")
            .to_string();
        re = Regex::new(r"https://(?P<num>[0-9][0-9][0-9]).rfd.oxide.computer").unwrap();
        cleaned = re
            .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/0$num")
            .to_string();
        re = Regex::new(r"https://(?P<num>[0-9][0-9][0-9][0-9]).rfd.oxide.computer").unwrap();
        cleaned = re
            .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/$num")
            .to_string();

        self.0 = cleaned
            .replace("link:", &format!("link:https://{}.rfd.oxide.computer/", num))
            .replace(&format!("link:https://{}.rfd.oxide.computer/http", num), "link:http");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspects_content_for_asciidoc() {
        let content = RFDContent::new(
            r#"
:showtitle:
:toc: left
:numbered:
:icons: font
:state: published  
:discussion: https://github.com/company/repo/pull/123
:revremark: State: {state} | {discussion}
:authors: FirstName LastName <fname@company.org>
"#,
        );

        match content {
            Ok(RFDContent::Asciidoc(_)) => (),
            other => panic!("Invalid inspection result {:?}", other),
        }
    }

    #[test]
    fn test_inspects_content_for_markdown() {
        let examples = vec![
            r#"
---
authors: FirstName LastName <fname@company.org>
state: discussion   
discussion: https://github.com/company/repo/pull/123
---

# RFD 123"#,
        ];

        for example in examples {
            let content = RFDContent::new(example);

            match content {
                Ok(RFDContent::Markdown(_)) => (),
                other => panic!("Invalid inspection result {:?}", other),
            }
        }
    }

    #[test]
    fn test_inspect_fails_on_indeterminate_content() {
        let content = RFDContent::new(
            r#"
showtitle:
notreallystate: discussion

# RFD 123
= RFD 123"#,
        );

        assert!(content.is_err())
    }

    #[test]
    fn test_clean_rfd_html_links() {
        let content = r#"https://3.rfd.oxide.computer
        https://41.rfd.oxide.computer
        https://543.rfd.oxide.computer#-some-link
        https://3245.rfd.oxide.computer/things
        https://3265.rfd.oxide.computer/things
        <img src="things.png" \>
        <a href="\#_principles">
        <object data="thing.svg">
        <object type="image/svg+xml" data="thing.svg">
        <a href="\#things" \>
        link:thing.html[Our thing]
        link:http://example.com[our example]"#;

        let mut html = RFDHtml(content.to_string());

        html.clean_links("0032");

        let expected = r#"https://rfd.shared.oxide.computer/rfd/0003
        https://rfd.shared.oxide.computer/rfd/0041
        https://rfd.shared.oxide.computer/rfd/0543#-some-link
        https://rfd.shared.oxide.computer/rfd/3245/things
        https://rfd.shared.oxide.computer/rfd/3265/things
        <img src="/static/images/0032/things.png" \>
        <a href="/rfd/0032#_principles">
        <object data="/static/images/0032/thing.svg">
        <object type="image/svg+xml" data="/static/images/0032/thing.svg">
        <a href="/rfd/0032#things" \>
        link:https://0032.rfd.oxide.computer/thing.html[Our thing]
        link:http://example.com[our example]"#;

        assert_eq!(expected, html.0);
    }

    // Read authors tests

    #[test]
    fn test_get_markdown_authors() {
        let content = r#"sdfsdf
sdfsdf
authors: things, joe
dsfsdf
sdf
authors: nope"#;
        let authors = RFDContent::new_markdown(content).get_authors();
        let expected = "things, joe".to_string();
        assert_eq!(expected, authors);
    }

    #[test]
    fn test_get_markdown_ignores_asciidoc_authors() {
        let content = r#"sdfsdf
= sdfgsdfgsdfg
things, joe
dsfsdf
sdf
:authors: nope"#;
        let authors = RFDContent::new_markdown(content).get_authors();
        let expected = "".to_string();
        assert_eq!(expected, authors);
    }

    #[test]
    fn test_get_asciidoc_fallback_authors() {
        let content = r#"sdfsdf
= sdfgsdfgsdfg
things <things@email.com>, joe <joe@email.com>
dsfsdf
sdf
authors: nope"#;
        let authors = RFDContent::new_asciidoc(content).get_authors();
        let expected = r#"things <things@email.com>, joe <joe@email.com>"#.to_string();
        assert_eq!(expected, authors);
    }

    #[test]
    fn test_get_asciidoc_attribute_authors() {
        let content = r#":authors: Jess <jess@thing.com>
= sdfgsdfgsdfg
{authors}
dsfsdf
sdf"#;
        let authors = RFDContent::new_asciidoc(content).get_authors();
        let expected = r#"Jess <jess@thing.com>"#.to_string();
        assert_eq!(expected, authors);
    }

    // Read state tests

    #[test]
    fn test_get_markdown_state() {
        let content = r#"sdfsdf
sdfsdf
state: discussion
dsfsdf
sdf
authors: nope"#;
        let state = RFDContent::new_markdown(content).get_state();
        let expected = "discussion".to_string();
        assert_eq!(expected, state);
    }

    #[test]
    fn test_get_asciidoc_state() {
        let content = r#"sdfsdf
= sdfgsdfgsdfg
:state: prediscussion
dsfsdf
sdf
:state: nope"#;
        let state = RFDContent::new_asciidoc(content).get_state();
        let expected = "prediscussion".to_string();
        assert_eq!(expected, state);
    }

    // Read discussion link tests

    #[test]
    fn test_get_markdown_discussion_link() {
        let content = r#"sdfsdf
sdfsdf
discussion: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
authors: nope"#;
        let discussion = RFDContent::new_markdown(content).get_discussion();
        let expected = "https://github.com/oxidecomputer/rfd/pulls/1".to_string();
        assert_eq!(expected, discussion);
    }

    #[test]
    fn test_get_asciidoc_discussion_link() {
        let content = r#"sdfsdf
= sdfgsdfgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
:discussion: nope"#;
        let discussion = RFDContent::new_asciidoc(content).get_discussion();
        let expected = "https://github.com/oxidecomputer/rfd/pulls/1".to_string();
        assert_eq!(expected, discussion);
    }

    // Update discussion link tests

    #[test]
    fn test_update_existing_markdown_discussion_link() {
        let link = "https://github.com/oxidecomputer/rfd/pulls/2019";

        let content = r#"sdfsdf
        sdfsdf
        discussion:   https://github.com/oxidecomputer/rfd/pulls/1
        dsfsdf
        sdf
        authors: nope"#;
        let mut rfd = RFDContent::new_markdown(content);
        rfd.update_discussion_link(link);

        let expected = r#"sdfsdf
        sdfsdf
        discussion: https://github.com/oxidecomputer/rfd/pulls/2019
        dsfsdf
        sdf
        authors: nope"#;
        assert_eq!(expected, rfd.raw());
    }

    #[test]
    fn test_update_existing_asciidoc_discussion_link_and_ignores_markdown_link() {
        let link = "https://github.com/oxidecomputer/rfd/pulls/2019";

        let content = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
:discussion: nope"#;

        let mut rfd = RFDContent::new_asciidoc(content);
        rfd.update_discussion_link(link);
        let expected = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/2019
dsfsdf
sdf
:discussion: nope"#;
        assert_eq!(expected, rfd.raw());
    }

    #[test]
    fn test_update_missing_asciidoc_discussion_link_and_ignores_markdown_link() {
        let link = "https://github.com/oxidecomputer/rfd/pulls/2019";

        let content = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion:
dsfsdf
sdf
:discussion: nope"#;

        let mut rfd = RFDContent::new_asciidoc(content);
        rfd.update_discussion_link(link);
        let expected = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/2019
dsfsdf
sdf
:discussion: nope"#;
        assert_eq!(expected, rfd.raw());
    }

    // Update state tests

    #[test]
    fn test_update_existing_markdown_state() {
        let state = "discussion";
        let content = r#"sdfsdf
sdfsdf
state:   sdfsdfsdf
dsfsdf
sdf
authors: nope"#;
        let mut rfd = RFDContent::new_markdown(content);
        rfd.update_state(state);

        let expected = r#"sdfsdf
sdfsdf
state: discussion
dsfsdf
sdf
authors: nope"#;
        assert_eq!(expected, rfd.raw());
    }

    #[test]
    fn test_update_existing_asciidoc_state() {
        let state = "discussion";
        let content = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state: prediscussion
dsfsdf
sdf
:state: nope"#;
        let mut rfd = RFDContent::new_asciidoc(content);
        rfd.update_state(state);
        let expected = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state: discussion
dsfsdf
sdf
:state: nope"#;
        assert_eq!(expected, rfd.raw());
    }

    #[test]
    fn test_update_empty_asciidoc_state() {
        let state = "discussion";
        let content = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state:
dsfsdf
sdf
:state: nope"#;
        let mut rfd = RFDContent::new_asciidoc(content);
        rfd.update_state(state);
        let expected = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state: discussion
dsfsdf
sdf
:state: nope"#;
        assert_eq!(expected, rfd.raw());
    }

    // Read title tests

    #[test]
    fn test_get_markdown_title() {
        let content = r#"things
# RFD 43 Identity and Access Management (IAM)
sdfsdf
title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
authors: nope"#;
        let rfd = RFDContent::new_markdown(content);
        let expected = "Identity and Access Management (IAM)".to_string();
        assert_eq!(expected, rfd.get_title());
    }

    #[test]
    fn test_get_asciidoc_title() {
        let content = r#"sdfsdf
= RFD 43 Identity and Access Management (IAM)
:title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
= RFD 53 Bye
sdf
:title: nope"#;
        let rfd = RFDContent::new_asciidoc(content);
        let expected = "Identity and Access Management (IAM)".to_string();
        assert_eq!(expected, rfd.get_title());
    }

    #[test]
    fn test_get_asciidoc_title_without_rfd_prefix() {
        // Add a test to show what happens for rfd 31 where there is no "RFD" in
        // the title.
        let content = r#"sdfsdf
= Identity and Access Management (IAM)
:title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
:title: nope"#;
        let rfd = RFDContent::new_asciidoc(content);
        let expected = "Identity and Access Management (IAM)".to_string();
        assert_eq!(expected, rfd.get_title());
    }

    fn test_rfd_content() -> &'static str {
        r#"
:showtitle:
:toc: left
:numbered:
:icons: font
:state: prediscussion
:revremark: State: {state}
:docdatetime: 2019-01-04 19:26:06 UTC
:localdatetime: 2019-01-04 19:26:06 UTC

= RFD 123 Place
FirstName LastName <fname@company.org>

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nunc et dignissim nisi. Donec ut libero in 
dolor tempor aliquam quis quis nisl. Proin sit amet nunc in orci suscipit placerat. Mauris 
pellentesque fringilla lacus id gravida. Donec in velit luctus, elementum mauris eu, pellentesque 
massa. In lectus orci, vehicula at aliquet nec, elementum eu nisi. Vivamus viverra imperdiet 
malesuada.

. Suspendisse blandit sem ligula, ac luctus metus condimentum non. Fusce enim purus, tincidunt ut 
tortor eget, sollicitudin vestibulum sem. Proin eu velit orci.

. Proin eu finibus velit. Morbi eget blandit neque.

```mermaid
graph TD;
    A-->B;
    A-->C;
    B-->D;
    C-->D;
```

. Maecenas molestie, quam nec lacinia porta, lectus turpis molestie quam, at fringilla neque ipsum 
in velit.

. Donec elementum luctus mauris.
"#
    }

    #[tokio::test]
    async fn test_asciidoc_to_html() {
        let _ = env_logger::builder().is_test(true).try_init();

        let rfd = RFDAsciidoc::new(Cow::Borrowed(test_rfd_content()));
        let expected = "<h1>RFD 123 Place</h1>\n<div class=\"paragraph\">\n<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nunc et dignissim nisi. Donec ut libero in\ndolor tempor aliquam quis quis nisl. Proin sit amet nunc in orci suscipit placerat. Mauris\npellentesque fringilla lacus id gravida. Donec in velit luctus, elementum mauris eu, pellentesque\nmassa. In lectus orci, vehicula at aliquet nec, elementum eu nisi. Vivamus viverra imperdiet\nmalesuada.</p>\n</div>\n<div class=\"olist arabic\">\n<ol class=\"arabic\">\n<li>\n<p>Suspendisse blandit sem ligula, ac luctus metus condimentum non. Fusce enim purus, tincidunt ut\ntortor eget, sollicitudin vestibulum sem. Proin eu velit orci.</p>\n</li>\n<li>\n<p>Proin eu finibus velit. Morbi eget blandit neque.</p>\n</li>\n</ol>\n</div>\n<div class=\"listingblock\">\n<div class=\"content\">\n<pre class=\"highlight\"><code class=\"language-mermaid\" data-lang=\"mermaid\">graph TD;\n    A--&gt;B;\n    A--&gt;C;\n    B--&gt;D;\n    C--&gt;D;</code></pre>\n</div>\n</div>\n<div class=\"olist arabic\">\n<ol class=\"arabic\">\n<li>\n<p>Maecenas molestie, quam nec lacinia porta, lectus turpis molestie quam, at fringilla neque ipsum\nin velit.</p>\n</li>\n<li>\n<p>Donec elementum luctus mauris.</p>\n</li>\n</ol>\n</div>\n";

        assert_eq!(
            expected,
            from_utf8(&rfd.parse(RFDOutputFormat::Html).await.unwrap()).unwrap()
        );
    }

    // TODO: Find a way to generate a reproducable PDF across systems
    #[ignore]
    #[tokio::test]
    async fn test_asciidoc_to_pdf() {
        let _ = env_logger::builder().is_test(true).try_init();

        let rfd = RFDAsciidoc::new(Cow::Borrowed(test_rfd_content()));
        let pdf = rfd.parse(RFDOutputFormat::Pdf).await.unwrap();

        let ref_path = format!(
            "{}/tests/ref/asciidoc_to_pdf.pdf",
            std::env::var("CARGO_MANIFEST_DIR").unwrap()
        );
        let expected = std::fs::read(&ref_path).unwrap();

        assert_eq!(expected, pdf);
    }
}
