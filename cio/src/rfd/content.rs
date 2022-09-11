use anyhow::{bail, Result};
use comrak::{markdown_to_html, ComrakOptions};
use log::info;
use regex::Regex;
use std::{borrow::Cow, env, fs, path::PathBuf, process::Command, str::from_utf8};
use uuid::Uuid;

use super::{GitHubRFDBranch, RFDNumber, RFDPdf};
use crate::utils::{decode_base64, write_file};

pub enum RFDContent<'a> {
    Asciidoc(RFDAsciidoc<'a>),
    Markdown(RFDMarkdown<'a>),
}

impl<'a> RFDContent<'a> {
    /// Create a new RFDContent wrapper and attempt to determine the type by examining the source
    /// contents.
    // TODO: This content inspection should be replaced by actually storing the format of the
    // content in the RFD struct
    pub fn new<T>(content: T) -> Result<Self>
    where
        T: Into<Cow<'a, str>>,
    {
        let content = content.into();

        let is_asciidoc = {
            // A regular expression that looks for commonly used Asciidoc attributes. These are static
            // regular expressions and can be safely unwrapped.
            let attribute_check = Regex::new(r"^(:showtitle:|:numbered:|:toc: left|:icons: font)$").unwrap();
            let state_check =
                Regex::new(r"^:state: (ideation|prediscussion|discussion|abandoned|published|committed)$").unwrap();

            // Check that the content contains at least one of the commonly used asciidoc attributes,
            // and contains a state line.
            attribute_check.is_match(&content) && state_check.is_match(&content)
        };

        let is_markdown = !is_asciidoc && {
            let title_check = Regex::new(r"^# RFD").unwrap();
            let state_check =
                Regex::new(r"^:state: (ideation|prediscussion|discussion|abandoned|published|committed)$").unwrap();

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

    pub fn new_asciidoc(content: Cow<'a, str>) -> Self {
        Self::Asciidoc(RFDAsciidoc::new(content))
    }

    pub fn new_markdown(content: Cow<'a, str>) -> Self {
        Self::Markdown(RFDMarkdown::new(content))
    }

    pub fn raw(&self) -> &str {
        match self {
            Self::Asciidoc(adoc) => &adoc.content,
            Self::Markdown(md) => &md.content,
        }
    }

    pub fn into_inner(self) -> String {
        match self {
            Self::Asciidoc(adoc) => adoc.content.into_owned(),
            Self::Markdown(md) => md.content.into_owned(),
        }
    }

    pub async fn to_html(&self, number: &RFDNumber, branch: &GitHubRFDBranch) -> Result<RFDHtml> {
        match self {
            Self::Asciidoc(adoc) => adoc.to_html(number, branch).await,
            Self::Markdown(md) => md.to_html(number),
        }
    }

    pub async fn to_pdf(&self, title: &str, number: &RFDNumber, branch: &GitHubRFDBranch) -> Result<RFDPdf> {
        match self {
            Self::Asciidoc(adoc) => adoc.to_pdf(title, number, branch).await,
            _ => Err(anyhow::anyhow!("Only asciidoc supports PDF generation")),
        }
    }

    pub fn update_discussion_link(&mut self, link: &str) {
        let (re, pre, content) = match self {
            RFDContent::Asciidoc(ref mut adoc) => (
                Regex::new(r"(?m)(:discussion:.*$)").unwrap(),
                ":",
                adoc.content.to_mut(),
            ),
            RFDContent::Markdown(ref mut md) => (Regex::new(r"(?m)(discussion:.*$)").unwrap(), "", md.content.to_mut()),
        };

        let replacement = if let Some(v) = re.find(&content) {
            v.as_str().to_string()
        } else {
            String::new()
        };

        *content = content.replacen(&replacement, &format!("{}discussion: {}", pre, link.trim()), 1);
    }

    pub fn update_state(&mut self, state: &str) {
        let (re, pre, content) = match self {
            RFDContent::Asciidoc(ref mut adoc) => {
                (Regex::new(r"(?m)(:state:.*$)").unwrap(), ":", adoc.content.to_mut())
            }
            RFDContent::Markdown(ref mut md) => (Regex::new(r"(?m)(state:.*$)").unwrap(), "", md.content.to_mut()),
        };

        let replacement = if let Some(v) = re.find(&content) {
            v.as_str().to_string()
        } else {
            String::new()
        };

        *content = content.replacen(&replacement, &format!("{}state: {}", pre, state.trim()), 1);
    }

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
                if c.is_none() {
                    // If we couldn't find anything assume we have no title.
                    // This was related to this error in Sentry:
                    // https://sentry.io/organizations/oxide-computer-company/issues/2701636092/?project=-1
                    String::new()
                } else {
                    let results = c.unwrap();

                    results
                        .as_str()
                        .replace("RFD", "")
                        .replace("# ", "")
                        .replace("= ", " ")
                        .trim()
                        .to_string()
                }
            }
        }
    }

    pub fn get_state(&self) -> String {
        let re = Regex::new(r"(?m)(state:.*$)").unwrap();

        match re.find(self.raw()) {
            Some(v) => v.as_str().replace("state:", "").trim().to_string(),
            None => Default::default(),
        }
    }

    pub fn get_discussion(&self) -> String {
        let re = Regex::new(r"(?m)(discussion:.*$)").unwrap();
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

    pub fn get_authors(&self) -> String {
        match self {
            Self::Asciidoc(RFDAsciidoc { content, .. }) => {
                // We must have asciidoc content.
                // We want to find the line under the first "=" line (which is the title), authors is under
                // that.
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

    pub async fn to_html(&self, number: &RFDNumber, branch: &GitHubRFDBranch) -> Result<RFDHtml> {
        self.download_images(number, branch).await?;

        let mut html = RFDHtml(from_utf8(&self.parse(RFDAsciidocOutputFormat::Html).await?)?.to_string());
        html.clean_links(&number.as_number_string());

        if let Err(err) = self.cleanup_tmp_path() {
            log::error!("Failed to clean up temporary working files for {:?} {:?}", number, err);
        }

        Ok(html)
    }

    pub async fn to_pdf(&self, title: &str, number: &RFDNumber, branch: &GitHubRFDBranch) -> Result<RFDPdf> {
        self.download_images(number, branch).await?;

        let content = self.parse(RFDAsciidocOutputFormat::Pdf).await?;

        if let Err(err) = self.cleanup_tmp_path() {
            log::error!("Failed to clean up temporary working files for {:?} {:?}", number, err);
        }

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

    async fn parse(&self, format: RFDAsciidocOutputFormat) -> Result<Vec<u8>> {
        info!("[asciidoc] Parsing asciidoc file");

        // Create the path to the local tmp file for holding the asciidoc contents
        let storage_path = self.tmp_path();
        let file_path = storage_path.join("contents.adoc");

        // // Write the contents to a temporary file.
        write_file(&file_path, deunicode::deunicode(&self.content).as_bytes()).await?;

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

        info!("[asciidoc] Finished cleanup and returning");

        Ok(result)
    }

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
                image.path.replace(&dir, "").trim_start_matches('/')
            );

            write_file(&PathBuf::from(image_path), &decode_base64(&image.content)).await?;

            info!(
                "[asciidoc] Wrote embedded image to temp dir {} / {}",
                number, branch.branch
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
            fs::remove_dir_all(storage_path)?
        }

        Ok(())
    }
}

enum RFDAsciidocOutputFormat {
    Html,
    Pdf,
}

impl RFDAsciidocOutputFormat {
    pub fn command(&self, working_dir: &PathBuf, file_path: &PathBuf) -> Command {
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

pub struct RFDMarkdown<'a> {
    content: Cow<'a, str>,
}

impl<'a> RFDMarkdown<'a> {
    pub fn new(content: Cow<'a, str>) -> Self {
        Self { content }
    }

    pub fn to_html(&self, number: &RFDNumber) -> Result<RFDHtml> {
        let mut html = RFDHtml(markdown_to_html(&self.content, &ComrakOptions::default()));
        html.clean_links(&number.as_number_string());

        Ok(html)
    }
}

pub struct RFDHtml(pub String);

impl RFDHtml {
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
