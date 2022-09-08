use anyhow::{bail, Result};
use comrak::{markdown_to_html, ComrakOptions};
use log::{info, warn};
use regex::Regex;
use std::{
    env,
    fs,
    path::PathBuf,
    process::Command,
    str::from_utf8
};
use uuid::Uuid;

use crate::utils::{
    decode_base64,
    write_file
};
use super::{
    GitHubRFDBranch,
    RFDPdf
};

pub enum RFDContent {
    Asciidoc(RFDAsciidoc),
    Markdown(RFDMarkdown),
}

impl RFDContent {
    pub fn new_asciidoc(content: String) -> Self {
        Self::Asciidoc(RFDAsciidoc::new(content))
    }

    pub fn new_markdown(content: String) -> Self {
        Self::Markdown(RFDMarkdown::new(content))
    }

    pub fn raw(&self) -> &str {
        match self {
            Self::Asciidoc(adoc) => adoc.content.as_str(),
            Self::Markdown(md) => md.content.as_str(),
        }
    }

    pub async fn to_html(
        &self,
        branch: &GitHubRFDBranch,
    ) -> Result<RFDHtml> {
        match self {
            Self::Asciidoc(adoc) => {
                adoc.to_html(branch).await
            }
            Self::Markdown(md) => {
                md.to_html(branch)
            }
        }
    }

    pub async fn to_pdf(
        &self,
        title: &str,
        branch: &GitHubRFDBranch,
    ) -> Result<RFDPdf> {
        match self {
            Self::Asciidoc(adoc) => {
                adoc.to_pdf(title, branch).await
            }
            _ => Err(anyhow::anyhow!("Only asciidoc supports PDF generation"))
        }
    }

    pub fn update_discussion_link(&mut self, link: &str) {
        let (mut re, mut pre, &mut content) = match self {
            RFDContent::Asciidoc(adoc) => {
                (Regex::new(r"(?m)(:discussion:.*$)").unwrap(), ":", &mut adoc.content)
            },
            RFDContent::Markdown(md) => {
                (Regex::new(r"(?m)(discussion:.*$)").unwrap(), "", &mut md.content)
            }
        };

        let replacement = if let Some(v) = re.find(&content) {
            v.as_str().to_string()
        } else {
            String::new()
        };
    
        content = content.replacen(&replacement, &format!("{}discussion: {}", pre, link.trim()), 1);
    }
    
    pub fn update_state(&mut self, state: &str, is_markdown: bool) {
        let (mut re, mut pre, &mut content) = match self {
            RFDContent::Asciidoc(adoc) => {
                (Regex::new(r"(?m)(:state:.*$)").unwrap(), ":", &mut adoc.content)
            },
            RFDContent::Markdown(md) => {
                (Regex::new(r"(?m)(state:.*$)").unwrap(), "", &mut md.content)
            }
        };
    
        let replacement = if let Some(v) = re.find(&content) {
            v.as_str().to_string()
        } else {
            String::new()
        };

        content = content.replacen(&replacement, &format!("{}state: {}", pre, state.trim()), 1);
    }
}

struct RFDAsciidoc {
    content: String,
    storage_id: Uuid
}

impl RFDAsciidoc {
    pub fn new(content: String) -> Self {
        Self { content, storage_id: Uuid::new_v4() }
    }

    pub async fn to_html(
        &self,
        branch: &GitHubRFDBranch,
    ) -> Result<RFDHtml> {
        self.download_images(branch).await?;

        let mut html = RFDHtml(from_utf8(&self.parse(RFDAsciidocOutputFormat::Html).await?)?.to_string());
        html.clean_links(&branch.rfd_number.as_number_string());

        if let Err(err) = self.cleanup_tmp_path() {
            log::error!("Failed to clean up temporary working files for {:?}", branch.rfd_number);
        }

        Ok(html)
    }

    pub async fn to_pdf(
        &self,
        title: &str,
        branch: &GitHubRFDBranch,
    ) -> Result<RFDPdf> {
        self.download_images(branch).await?;

        let content = self.parse(RFDAsciidocOutputFormat::Pdf).await?;

        if let Err(err) = self.cleanup_tmp_path() {
            log::error!("Failed to clean up temporary working files for {:?}", branch.rfd_number);
        }

        let filename = format!(
            "RFD {} {}.pdf",
            branch.rfd_number.as_number_string(),
            title.replace('/', "-").replace('\'', "").replace(':', "").trim()
        );

        Ok(RFDPdf {
            filename,
            contents: content,
            number: branch.rfd_number.clone(),
        })
    }

    async fn parse(&self, format: RFDAsciidocOutputFormat) -> Result<Vec<u8>> {
        // info!(
        //     "[asciidoc] Parsing asciidoc file {} / {} / {}",
        //     self.id, self.number, branch
        // );

        // Create the path to the local tmp file for holding the asciidoc contents
        let storage_path = self.tmp_path();
        let file_path = storage_path.join("contents.adoc");

        // // Write the contents to a temporary file.
        write_file(&file_path, deunicode::deunicode(&self.content).as_bytes()).await?;

        // info!(
        //     "[asciidoc] Wrote file to temp dir {} / {} / {}",
        //     self.id, self.number, branch
        // );

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

        // info!(
        //     "[asciidoc] Completed asciidoc rendering {} / {} / {}",
        //     self.id, self.number, branch
        // );

        let result = if cmd_output.status.success() {
            cmd_output.stdout
        } else {
            bail!(
                "[rfds] running asciidoctor failed: {} {}",
                from_utf8(&cmd_output.stdout)?,
                from_utf8(&cmd_output.stderr)?
            );
        };

        // info!(
        //     "[asciidoc] Finished cleanup and returning {} / {} / {}",
        //     self.id, self.number, branch
        // );

        Ok(result)
    }

    async fn download_images(&self, branch: &GitHubRFDBranch) -> Result<()> {
        let dir = branch.repo_directory();
        let storage_path = self.tmp_path();
        let storage_path_string = storage_path.to_str().ok_or_else(|| anyhow::anyhow!("Unable to convert image temp storage path to string"))?;
        let images = branch.get_images(&dir).await?;

        for image in images {
            // Save the image to our temporary directory.
            let image_path = format!("{}/{}", storage_path_string, image.path.replace(&dir, "").trim_start_matches('/'));

            write_file(&PathBuf::from(image_path), &decode_base64(&image.content)).await?;

            info!(
                "[asciidoc] Wrote embedded image to temp dir {} / {}",
                branch.rfd_number, branch.branch
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
    Pdf
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
                let command = Command::new("asciidoctor-pdf");
                command
                    .current_dir(working_dir)
                    .args(&[
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

struct RFDMarkdown {
    content: String,
}

impl RFDMarkdown {
    pub fn new(content: String) -> Self {
        Self { content }
    }

    pub fn to_html(&self, branch: &GitHubRFDBranch) -> Result<RFDHtml> {
        let mut html = RFDHtml(markdown_to_html(&self.content, &ComrakOptions::default()));
        html.clean_links(&branch.rfd_number.as_number_string())?;

        Ok(html)
    }
}

pub struct RFDHtml(pub String);

impl RFDHtml {
    pub fn clean_links(&mut self, num: &str) -> Result<()> {
        let mut cleaned = self.0
            .replace(r#"href="\#"#, &format!(r#"href="/rfd/{}#"#, num))
            .replace("href=\"#", &format!("href=\"/rfd/{}#", num))
            .replace(r#"img src=""#, &format!(r#"img src="/static/images/{}/"#, num))
            .replace(r#"object data=""#, &format!(r#"object data="/static/images/{}/"#, num))
            .replace(
                r#"object type="image/svg+xml" data=""#,
                &format!(r#"object type="image/svg+xml" data="/static/images/{}/"#, num),
            );

        let mut re = Regex::new(r"https://(?P<num>[0-9]).rfd.oxide.computer")?;
        cleaned = re
            .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/000$num")
            .to_string();
        re = Regex::new(r"https://(?P<num>[0-9][0-9]).rfd.oxide.computer")?;
        cleaned = re
            .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/00$num")
            .to_string();
        re = Regex::new(r"https://(?P<num>[0-9][0-9][0-9]).rfd.oxide.computer")?;
        cleaned = re
            .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/0$num")
            .to_string();
        re = Regex::new(r"https://(?P<num>[0-9][0-9][0-9][0-9]).rfd.oxide.computer")?;
        cleaned = re
            .replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/$num")
            .to_string();

        self.0 = cleaned
            .replace("link:", &format!("link:https://{}.rfd.oxide.computer/", num))
            .replace(&format!("link:https://{}.rfd.oxide.computer/http", num), "link:http");

        Ok(())
    }
}
