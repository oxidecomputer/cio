use std::collections::BTreeMap;
use std::str::from_utf8;

use comrak::{markdown_to_html, ComrakOptions};
use csv::ReaderBuilder;
use futures_util::TryStreamExt;
use hubcaps::repositories::Repository;
use hubcaps::Github;
use regex::Regex;
use tracing::instrument;

use crate::db::Database;
use crate::models::NewRFD;
use crate::utils::{create_or_update_file_in_github_repo, github_org};

/// Get the RFDs from the rfd GitHub repo.
#[instrument]
#[inline]
pub async fn get_rfds_from_repo(github: &Github) -> BTreeMap<i32, NewRFD> {
    let repo = github.repo(github_org(), "rfd");
    let r = repo.get().await.unwrap();

    // Get the contents of the .helpers/rfd.csv file.
    let rfd_csv_content = repo.content().file("/.helpers/rfd.csv", &r.default_branch).await.expect("failed to get rfd csv content").content;
    let rfd_csv_string = from_utf8(&rfd_csv_content).unwrap();

    // Create the csv reader.
    let mut csv_reader = ReaderBuilder::new().delimiter(b',').has_headers(true).from_reader(rfd_csv_string.as_bytes());

    // Create the BTreeMap of RFDs.
    let mut rfds: BTreeMap<i32, NewRFD> = Default::default();
    for r in csv_reader.deserialize() {
        let mut rfd: NewRFD = r.unwrap();

        // TODO: this whole thing is a mess jessfraz needs to cleanup
        rfd.number_string = NewRFD::generate_number_string(rfd.number);
        rfd.name = NewRFD::generate_name(rfd.number, &rfd.title);

        // Add this to our BTreeMap.
        rfds.insert(rfd.number, rfd);
    }

    rfds
}

/// Try to get the markdown or asciidoc contents from the repo.
#[instrument]
#[inline]
pub async fn get_rfd_contents_from_repo(github: &Github, branch: &str, dir: &str) -> (String, bool, String) {
    let repo = github.repo(github_org(), "rfd");
    let r = repo.get().await.unwrap();
    let repo_contents = repo.content();
    let mut is_markdown = false;
    let decoded: String;
    let sha: String;

    // Get the contents of the file.
    let path = format!("{}/README.adoc", dir);
    match repo_contents.file(&path, branch).await {
        Ok(contents) => {
            decoded = from_utf8(&contents.content).unwrap().trim().to_string();
            sha = contents.sha;
        }
        Err(e) => {
            println!("[rfd] getting file contents for {} failed: {}, trying markdown instead...", path, e);

            // Try to get the markdown instead.
            is_markdown = true;
            let contents = repo_contents.file(&format!("{}/README.md", dir), branch).await.unwrap();

            decoded = from_utf8(&contents.content).unwrap().trim().to_string();
            sha = contents.sha;
        }
    }

    // Get all the images in the branch and make sure they are in the images directory on master.
    let images = get_images_in_branch(&repo, dir, branch).await;
    for image in images {
        let new_path = image.path.replace("rfd/", "src/public/static/images/");
        // Make sure we have this file in the static images dir on the master branch.
        create_or_update_file_in_github_repo(&repo, &r.default_branch, &new_path, image.content.to_vec()).await;
    }

    (decoded, is_markdown, sha)
}

// Get all the images in a specific directory of a GitHub branch.
#[instrument(skip(repo))]
#[inline]
pub async fn get_images_in_branch(repo: &Repository, dir: &str, branch: &str) -> Vec<hubcaps::content::File> {
    let mut files: Vec<hubcaps::content::File> = Default::default();

    // Get all the images in the branch and make sure they are in the images directory on master.
    for file in repo.content().iter(dir, branch).try_collect::<Vec<hubcaps::content::DirectoryItem>>().await.unwrap() {
        if is_image(&file.name) {
            // Get the contents of the image.
            match repo.content().file(&file.path, branch).await {
                Ok(f) => {
                    // Push the file to our vector.
                    files.push(f);
                }
                Err(e) => match e {
                    hubcaps::errors::Error::Fault { code: _, ref error } => {
                        if error.message.contains("too_large") {
                            // The file is too big for us to get it's contents through this API.
                            // The error suggests we use the Git Data API but we need the file sha for
                            // that.
                            // We have the sha we can see if the files match using the
                            // Git Data API.
                            let blob = repo.git().blob(&file.sha).await.unwrap();
                            // Base64 decode the contents.
                            // TODO: move this logic to hubcaps.
                            let v = blob.content.replace("\n", "");
                            let decoded = base64::decode_config(&v, base64::STANDARD).unwrap();

                            // Push the new file.
                            files.push(hubcaps::content::File {
                                encoding: hubcaps::content::Encoding::Base64,
                                size: file.size,
                                name: file.name,
                                path: file.path,
                                content: hubcaps::content::DecodedContents(decoded.to_vec()),
                                sha: file.sha,
                                url: file.url,
                                git_url: file.git_url,
                                html_url: file.html_url,
                                download_url: file.download_url.unwrap_or_default(),
                                _links: file._links,
                            });

                            continue;
                        }
                        println!("[rfd] getting file contents for {} failed: {}", file.path, e);
                    }
                    _ => println!("[rfd] getting file contents for {} failed: {}", file.path, e),
                },
            }
        }
    }

    files
}

#[instrument]
#[inline]
pub fn parse_markdown(content: &str) -> String {
    markdown_to_html(content, &ComrakOptions::default())
}

/// Return if the file is an image.
#[instrument]
#[inline]
pub fn is_image(file: &str) -> bool {
    file.ends_with(".svg") || file.ends_with(".png") || file.ends_with(".jpg") || file.ends_with(".jpeg")
}

#[instrument]
#[inline]
pub fn clean_rfd_html_links(content: &str, num: &str) -> String {
    let mut cleaned = content
        .replace(r#"href="\#"#, &format!(r#"href="/rfd/{}#"#, num))
        .replace("href=\"#", &format!("href=\"/rfd/{}#", num))
        .replace(r#"img src=""#, &format!(r#"img src="/static/images/{}/"#, num))
        .replace(r#"object data=""#, &format!(r#"object data="/static/images/{}/"#, num))
        .replace(r#"object type="image/svg+xml" data=""#, &format!(r#"object type="image/svg+xml" data="/static/images/{}/"#, num));

    let mut re = Regex::new(r"https://(?P<num>[0-9]).rfd.oxide.computer").unwrap();
    cleaned = re.replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/000$num").to_string();
    re = Regex::new(r"https://(?P<num>[0-9][0-9]).rfd.oxide.computer").unwrap();
    cleaned = re.replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/00$num").to_string();
    re = Regex::new(r"https://(?P<num>[0-9][0-9][0-9]).rfd.oxide.computer").unwrap();
    cleaned = re.replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/0$num").to_string();
    re = Regex::new(r"https://(?P<num>[0-9][0-9][0-9][0-9]).rfd.oxide.computer").unwrap();
    cleaned = re.replace_all(&cleaned, "https://rfd.shared.oxide.computer/rfd/$num").to_string();

    cleaned
}

#[instrument]
#[inline]
pub fn update_discussion_link(content: &str, link: &str, is_markdown: bool) -> String {
    // TODO: there is probably a better way to do these regexes.
    let mut re = Regex::new(r"(?m)(:discussion:.*$)").unwrap();
    // Asciidoc starts with a colon.
    let mut pre = ":";
    if is_markdown {
        // Markdown does not start with a colon.
        pre = "";
        re = Regex::new(r"(?m)(discussion:.*$)").unwrap();
    }

    let replacement = if let Some(v) = re.find(&content) { v.as_str().to_string() } else { String::new() };

    content.replacen(&replacement, &format!("{}discussion: {}", pre, link.trim()), 1)
}

#[instrument]
#[inline]
pub fn update_state(content: &str, state: &str, is_markdown: bool) -> String {
    // TODO: there is probably a better way to do these regexes.
    let mut re = Regex::new(r"(?m)(:state:.*$)").unwrap();
    // Asciidoc starts with a colon.
    let mut pre = ":";
    if is_markdown {
        // Markdown does not start with a colon.
        pre = "";
        re = Regex::new(r"(?m)(state:.*$)").unwrap();
    }

    let replacement = if let Some(v) = re.find(&content) { v.as_str().to_string() } else { String::new() };

    content.replacen(&replacement, &format!("{}state: {}", pre, state.trim()), 1)
}

// Sync the rfds with our database.
#[instrument]
#[inline]
pub async fn refresh_db_rfds(github: &Github) {
    let rfds = get_rfds_from_repo(github).await;

    // Initialize our database.
    let db = Database::new();

    // Sync rfds.
    for (_, rfd) in rfds {
        let mut new_rfd = db.upsert_rfd(&rfd);

        // Expand the fields in the RFD.
        new_rfd.expand(github).await;

        // Update the RFD again.
        // We do this so the expand functions are only one place.
        db.update_rfd(&new_rfd);
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Database;
    use crate::models::{NewRFD, RFDs};
    use crate::rfds::{clean_rfd_html_links, refresh_db_rfds, update_discussion_link, update_state};
    use crate::utils::authenticate_github_jwt;

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_cron_rfds() {
        let github = authenticate_github_jwt();
        refresh_db_rfds(&github).await;
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
        <a href="\#things" \>"#;

        let cleaned = clean_rfd_html_links(&content, "0032");

        let expected = r#"https://rfd.shared.oxide.computer/rfd/0003
        https://rfd.shared.oxide.computer/rfd/0041
        https://rfd.shared.oxide.computer/rfd/0543#-some-link
        https://rfd.shared.oxide.computer/rfd/3245/things
        https://rfd.shared.oxide.computer/rfd/3265/things
        <img src="/static/images/0032/things.png" \>
        <a href="/rfd/0032#_principles">
        <object data="/static/images/0032/thing.svg">
        <object type="image/svg+xml" data="/static/images/0032/thing.svg">
        <a href="/rfd/0032#things" \>"#;

        assert_eq!(expected, cleaned);
    }

    #[test]
    fn test_get_authors() {
        let mut content = r#"sdfsdf
sdfsdf
authors: things, joe
dsfsdf
sdf
authors: nope"#;
        let mut authors = NewRFD::get_authors(&content, true);
        let mut expected = "things, joe".to_string();
        assert_eq!(expected, authors);

        content = r#"sdfsdf
= sdfgsdfgsdfg
things, joe
dsfsdf
sdf
:authors: nope"#;
        authors = NewRFD::get_authors(&content, true);
        assert_eq!(expected, authors);

        content = r#"sdfsdf
= sdfgsdfgsdfg
things <things@email.com>, joe <joe@email.com>
dsfsdf
sdf
authors: nope"#;
        authors = NewRFD::get_authors(&content, false);
        expected = r#"things <things@email.com>, joe <joe@email.com>"#.to_string();
        assert_eq!(expected, authors);
    }

    #[test]
    fn test_get_state() {
        let mut content = r#"sdfsdf
sdfsdf
state: discussion
dsfsdf
sdf
authors: nope"#;
        let mut state = NewRFD::get_state(&content);
        let mut expected = "discussion".to_string();
        assert_eq!(expected, state);

        content = r#"sdfsdf
= sdfgsdfgsdfg
:state: prediscussion
dsfsdf
sdf
:state: nope"#;
        state = NewRFD::get_state(&content);
        expected = "prediscussion".to_string();
        assert_eq!(expected, state);
    }

    #[test]
    fn test_get_discussion() {
        let mut content = r#"sdfsdf
sdfsdf
discussion: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
authors: nope"#;
        let mut discussion = NewRFD::get_discussion(&content);
        let expected = "https://github.com/oxidecomputer/rfd/pulls/1".to_string();
        assert_eq!(expected, discussion);

        content = r#"sdfsdf
= sdfgsdfgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
:discussion: nope"#;
        discussion = NewRFD::get_discussion(&content);
        assert_eq!(expected, discussion);
    }

    #[test]
    fn test_update_discussion_link() {
        let link = "https://github.com/oxidecomputer/rfd/pulls/2019";
        let mut content = r#"sdfsdf
sdfsdf
discussion:   https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
authors: nope"#;
        let mut result = update_discussion_link(&content, &link, true);
        let mut expected = r#"sdfsdf
sdfsdf
discussion: https://github.com/oxidecomputer/rfd/pulls/2019
dsfsdf
sdf
authors: nope"#;
        assert_eq!(expected, result);

        content = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
:discussion: nope"#;
        result = update_discussion_link(&content, &link, false);
        expected = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/2019
dsfsdf
sdf
:discussion: nope"#;
        assert_eq!(expected, result);

        content = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion:
dsfsdf
sdf
:discussion: nope"#;
        result = update_discussion_link(&content, &link, false);
        expected = r#"sdfsdf
= sdfgsd
discussion: fgsdfg
:discussion: https://github.com/oxidecomputer/rfd/pulls/2019
dsfsdf
sdf
:discussion: nope"#;
        assert_eq!(expected, result);
    }

    #[test]
    fn test_update_state() {
        let state = "discussion";
        let mut content = r#"sdfsdf
sdfsdf
state:   sdfsdfsdf
dsfsdf
sdf
authors: nope"#;
        let mut result = update_state(&content, &state, true);
        let mut expected = r#"sdfsdf
sdfsdf
state: discussion
dsfsdf
sdf
authors: nope"#;
        assert_eq!(expected, result);

        content = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state: prediscussion
dsfsdf
sdf
:state: nope"#;
        result = update_state(&content, &state, false);
        expected = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state: discussion
dsfsdf
sdf
:state: nope"#;
        assert_eq!(expected, result);

        content = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state:
dsfsdf
sdf
:state: nope"#;
        result = update_state(&content, &state, false);
        expected = r#"sdfsdf
= sdfgsd
state: fgsdfg
:state: discussion
dsfsdf
sdf
:state: nope"#;
        assert_eq!(expected, result);
    }

    #[test]
    fn test_get_title() {
        let mut content = r#"things
# RFD 43 Identity and Access Management (IAM)
sdfsdf
title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
authors: nope"#;
        let mut title = NewRFD::get_title(&content);
        let expected = "Identity and Access Management (IAM)".to_string();
        assert_eq!(expected, title);

        content = r#"sdfsdf
= RFD 43 Identity and Access Management (IAM)
:title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
= RFD 53 Bye
sdf
:title: nope"#;
        title = NewRFD::get_title(&content);
        assert_eq!(expected, title);

        // Add a test to show what happens for rfd 31 where there is no "RFD" in
        // the title.
        content = r#"sdfsdf
= Identity and Access Management (IAM)
:title: https://github.com/oxidecomputer/rfd/pulls/1
dsfsdf
sdf
:title: nope"#;
        title = NewRFD::get_title(&content);
        assert_eq!(expected, title);
    }

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_cron_rfds_airtable() {
        // Initialize our database.
        let db = Database::new();

        let rfds = db.get_rfds();
        // Update rfds in airtable.
        RFDs(rfds).update_airtable().await;
    }
}
