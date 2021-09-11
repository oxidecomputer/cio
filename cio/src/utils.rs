use std::{
    collections::{BTreeMap, HashMap},
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    str::from_utf8,
};

use anyhow::{anyhow, bail, Result};
use log::info;
use octorust::Client as GitHub;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use reqwest::get;
use sentry::IntoDsn;
use serde_json::Value;
use sodiumoxide::crypto::secretbox;

use crate::companies::Company;

/// Write a file.
pub fn write_file(file: &Path, contents: &[u8]) -> Result<()> {
    // create each directory.
    fs::create_dir_all(file.parent().unwrap())?;

    // Write to the file.
    let mut f = fs::File::create(file.to_path_buf())?;
    f.write_all(contents)?;

    info!("wrote file: {}", file.to_str().unwrap());

    Ok(())
}

/// Create a comment on a commit for a repo.
/// We use this a lot if a webhook was a success or errored.
pub async fn add_comment_to_commit(
    github: &GitHub,
    owner: &str,
    repo: &str,
    commit_sha: &str,
    message: &str,
) -> Result<()> {
    // TODO: check if we already left a comment.
    github
        .repos()
        .create_commit_comment(
            owner,
            repo,
            commit_sha,
            &octorust::types::ReposCreateCommitCommentRequest {
                body: message.to_string(),
                line: 0,
                path: String::new(),
                position: 0,
            },
        )
        .await?;

    Ok(())
}

/// Check if a GitHub issue already exists.
pub fn check_if_github_issue_exists(
    issues: &[octorust::types::IssueSimple],
    search: &str,
) -> Option<octorust::types::IssueSimple> {
    for i in issues {
        if i.title.contains(search) {
            return Some(i.clone());
        }
    }

    None
}

/// Return a user's public ssh key's from GitHub by their GitHub handle.
pub async fn get_github_user_public_ssh_keys(handle: &str) -> Result<Vec<String>> {
    let body = get(&format!("https://github.com/{}.keys", handle))
        .await?
        .text()
        .await?;

    Ok(body
        .lines()
        .filter_map(|key| {
            let kt = key.trim();
            if !kt.is_empty() {
                Some(kt.to_string())
            } else {
                None
            }
        })
        .collect())
}

/// Get a files content from a repo.
/// It returns a tuple of the bytes of the file content and the sha of the file.
pub async fn get_file_content_from_repo(
    github: &octorust::Client,
    owner: &str,
    repo: &str,
    branch: &str,
    path: &str,
) -> Result<(Vec<u8>, String)> {
    // Add the starting "/" so this works.
    // TODO: figure out why it doesn't work without it.
    let mut file_path = path.to_string();
    if !path.starts_with('/') {
        file_path = "/".to_owned() + path;
    }

    // Try to get the content for the file from the repo.
    match github.repos().get_content_file(owner, repo, &file_path, branch).await {
        Ok(file) => Ok((decode_base64(&file.content), file.sha.to_string())),
        Err(e) => {
            // TODO: better match on errors
            if e.to_string().contains("rate limit") {
                // We got a rate limit error.
                bail!("We got rate limited! {}", e);
            } else if e.to_string().contains("too large") {
                // The file is too big for us to get it's contents through this API.
                // The error suggests we use the Git Data API but we need the file sha for
                // that.
                // Get all the items in the directory and try to find our file and get the sha
                // for it so we can update it.
                let mut p = PathBuf::from(&file_path);
                p.pop();

                let files = github
                    .repos()
                    .get_content_vec_entries(owner, repo, p.to_str().unwrap(), branch)
                    .await?;
                for item in files {
                    if file_path.trim_start_matches('/') != item.path {
                        // Continue early.
                        continue;
                    }

                    // Otherwise, this is our file.
                    // We have the sha we can see if the files match using the
                    // Git Data API.
                    let blob = github.git().get_blob(owner, repo, &item.sha).await?;
                    // Base64 decode the contents.

                    return Ok((decode_base64(&blob.content), item.sha.to_string()));
                }

                bail!(
                    "[github content] Getting the file at {} on branch {} failed: {:?}",
                    file_path,
                    branch,
                    e
                );
            } else {
                bail!(
                    "[github content] Getting the file at {} on branch {} failed: {:?}",
                    file_path,
                    branch,
                    e
                );
            }
        }
    }
}

/// Create or update a file in a GitHub repository.
/// If the file does not exist, it will be created.
/// If the file exists, it will be updated _only if_ the content of the file has changed.
pub async fn create_or_update_file_in_github_repo(
    github: &octorust::Client,
    owner: &str,
    repo: &str,
    branch: &str,
    path: &str,
    new_content: Vec<u8>,
) -> Result<()> {
    let content = new_content.trim();
    // Add the starting "/" so this works.
    // TODO: figure out why it doesn't work without it.
    let mut file_path = path.to_string();
    if !path.starts_with('/') {
        file_path = "/".to_owned() + path;
    }

    // Try to get the content for the file from the repo.
    let (existing_content, sha) =
        if let Ok((e, s)) = get_file_content_from_repo(github, owner, repo, branch, path).await {
            (e, s)
        } else {
            (vec![], "".to_string())
        };

    if !existing_content.is_empty() || !sha.is_empty() {
        if content == existing_content {
            // They are the same so we can return early, we do not need to update the
            // file.
            info!("github file contents at {} are the same, no update needed", file_path);
            return Ok(());
        }

        // When the pdfs are generated they change the modified time that is
        // encoded in the file. We want to get that diff and see if it is
        // the only change so that we are not always updating those files.
        let diff = diffy::create_patch_bytes(&existing_content, &content);
        let bdiff = diff.to_bytes();
        let str_diff = from_utf8(&bdiff).unwrap_or("");
        if str_diff.contains("-/ModDate")
            && str_diff.contains("-/CreationDate")
            && str_diff.contains("+/ModDate")
            && str_diff.contains("-/CreationDate")
            && str_diff.contains("@@ -5,8 +5,8 @@")
        {
            // The binary contents are the same so we can return early.
            // The only thing that changed was the modified time and creation date.
            info!("github file contents at {} are the same, no update needed", file_path);
            return Ok(());
        }
    }

    // We need to create or update the file.
    match github
        .repos()
        .create_or_update_file_contents(
            owner,
            repo,
            file_path.trim_start_matches('/'),
            &octorust::types::ReposCreateUpdateFileContentsRequest {
                message: format!(
                    "Updating file content {} programatically\n\nThis is done from the cio repo \
                     utils::create_or_update_file function.",
                    file_path
                ),
                sha,
                branch: branch.to_string(),
                content: base64::encode(content),
                committer: Default::default(),
                author: Default::default(),
            },
        )
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            bail!(
                "[github content] updating file at {} on branch {} failed: {}",
                file_path,
                branch,
                e
            );
        }
    }
}

trait SliceExt {
    fn trim(&self) -> Self;
}

impl SliceExt for Vec<u8> {
    fn trim(&self) -> Vec<u8> {
        fn is_whitespace(c: &u8) -> bool {
            c == &b'\t' || c == &b' '
        }

        fn is_not_whitespace(c: &u8) -> bool {
            !is_whitespace(c)
        }

        if let Some(first) = self.iter().position(is_not_whitespace) {
            if let Some(last) = self.iter().rposition(is_not_whitespace) {
                self[first..last + 1].to_vec()
            } else {
                unreachable!();
            }
        } else {
            vec![]
        }
    }
}

pub fn default_date() -> chrono::naive::NaiveDate {
    chrono::naive::NaiveDate::parse_from_str("1970-01-01", "%Y-%m-%d").unwrap()
}

pub fn merge_json(a: &mut Value, b: Value) {
    match (a, b) {
        (a @ &mut Value::Object(_), Value::Object(b)) => {
            let a = a.as_object_mut().unwrap();
            for (k, v) in b {
                merge_json(a.entry(k).or_insert(Value::Null), v);
            }
        }
        (a @ &mut Value::Array(_), Value::Array(b)) => {
            let a = a.as_array_mut().unwrap();
            for v in b {
                a.push(v);
            }
        }
        (a, b) => *a = b,
    }
}

pub fn truncate(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        None => s.to_string(),
        Some((idx, _)) => s[..idx].to_string(),
    }
}

pub fn get_value(map: &HashMap<String, Vec<String>>, key: &str) -> String {
    let empty: Vec<String> = Default::default();
    let a = map.get(key).unwrap_or(&empty);

    if a.is_empty() {
        return Default::default();
    }

    a.get(0).unwrap().to_string()
}

pub fn decode_base64(c: &str) -> Vec<u8> {
    let v = c.replace("\n", "");
    let decoded = base64::decode(&v).unwrap();
    decoded.trim().to_vec()
}

pub fn decode_base64_to_string(c: &str) -> String {
    let decoded = decode_base64(c);
    from_utf8(&decoded).unwrap().trim().to_string()
}

pub async fn encrypt_github_secrets(
    github: &octorust::Client,
    company: &Company,
    repo: &str,
    s: &BTreeMap<String, String>,
) -> Result<(String, BTreeMap<String, String>)> {
    sodiumoxide::init().map_err(|_| anyhow!("initializing sodiumoxide failed!"))?;

    // Get the public key for the repo.
    let pk = github.actions().get_repo_public_key(&company.github_org, repo).await?;
    let pke = &base64::decode(pk.key)?;

    // Resize our slice.
    let mut spke: [u8; 32] = Default::default();
    for (i, v) in pke.iter().enumerate() {
        spke[i] = *v;
    }

    let mut secrets = s.clone();

    // Iterate over and encrypt all our secrets.
    for (name, secret) in secrets.clone() {
        // Load the key.
        let key = secretbox::Key(spke);
        let nonce = secretbox::gen_nonce();

        let ciphertext = secretbox::seal(secret.as_bytes(), &nonce, &key);
        let encoded = base64::encode(ciphertext);

        secrets.insert(name, encoded);
    }

    // Return our newly encrypted secrets and the key ID.
    Ok((pk.key_id, secrets))
}

/// Generate a random string that we can use as a temporary password for new users
/// when we set up their account.
pub fn generate_password() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(30)
        .map(char::from)
        .collect()
}

pub fn setup_logger() {
    // Initialize our logger.
    let mut log_builder = pretty_env_logger::formatted_builder();
    log_builder.parse_filters("info");
    log_builder.is_test(true);

    let logger = sentry_log::SentryLogger::with_dest(log_builder.build());

    log::set_boxed_logger(Box::new(logger)).unwrap_or_default();

    log::set_max_level(log::LevelFilter::Info);

    // Initialize sentry.
    let sentry_dsn = env::var("SENTRY_DSN").unwrap_or_default();

    if !sentry_dsn.is_empty() {
        let mut sha = env::var("GITHUB_SHA").unwrap_or_default();
        if !sha.is_empty() {
            sha = sha.chars().take(8).collect();
        }

        let _guard = sentry::init(sentry::ClientOptions {
            dsn: sentry_dsn.into_dsn().unwrap(),

            release: Some(sha.into()),
            environment: Some(
                env::var("SENTRY_ENV")
                    .unwrap_or_else(|_| "rust_test".to_string())
                    .into(),
            ),
            ..Default::default()
        });

        // Identify some items about the run here.
        sentry::configure_scope(|scope| {
            scope.set_tag("github.workflow", env::var("GITHUB_WORKFLOW").unwrap_or_default());
            scope.set_tag("github.actions", env::var("GITHUB_ACTIONS").unwrap_or_default());
            scope.set_tag("github.event.name", env::var("GITHUB_EVENT_NAME").unwrap_or_default());
            scope.set_tag("github.ref", env::var("GITHUB_REF").unwrap_or_default());
            scope.set_tag("github.sha", env::var("GITHUB_SHA").unwrap_or_default());
            scope.set_tag("runner.os", env::var("RUNNER_OS").unwrap_or_default());
            scope.set_tag("ci", env::var("CI").unwrap_or_default());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_utils() {
        // Get the company id for Oxide.
        // TODO: split this out per company.
        let db = crate::db::Database::new();
        let company = crate::companies::Company::get_from_db(&db, "Oxide".to_string()).unwrap();
        let github = company.authenticate_github().unwrap();

        let mut s: BTreeMap<String, String> = Default::default();
        s.insert("TESTING".to_string(), "thing".to_string());
        let secrets = encrypt_github_secrets(&github, &company, "cio", s).await.unwrap();
        println!("secrets: {:#?}", secrets);
    }
}
