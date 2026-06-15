use anyhow::{Result, bail, ensure};

use crate::model::is_valid_slug;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepoSpec {
    pub org: String,
    pub repo: String,
    pub clone_url: String,
}

pub fn resolve_repo_input(
    org: &str,
    repo: Option<&str>,
    repo_url: Option<&str>,
) -> Result<RepoSpec> {
    match (repo, repo_url) {
        (Some(_), Some(_)) => bail!("--repo and --repo-url are mutually exclusive"),
        (None, None) => bail!("missing repo input: pass --repo <REPO> or --repo-url <URL>"),
        (Some(repo), None) => {
            ensure!(is_valid_slug(org), "invalid GitHub owner: {org}");
            ensure!(is_valid_slug(repo), "invalid GitHub repo: {repo}");

            Ok(RepoSpec {
                org: org.to_owned(),
                repo: repo.to_owned(),
                clone_url: format!("git@github.com:{org}/{repo}.git"),
            })
        }
        (None, Some(repo_url)) => parse_github_repo_url(repo_url),
    }
}

pub fn parse_github_repo_url(raw_url: &str) -> Result<RepoSpec> {
    let mut url = raw_url.trim();
    url = url.split('#').next().unwrap_or(url);
    url = url.split('?').next().unwrap_or(url);
    url = url.trim_end_matches('/');
    url = url.strip_suffix(".git").unwrap_or(url);

    let path = if let Some(path) = url.strip_prefix("git@github.com:") {
        path
    } else if let Some(path) = url.strip_prefix("ssh://git@github.com/") {
        path
    } else if let Some(path) = url.strip_prefix("https://github.com/") {
        path
    } else if url.starts_with("git@") || url.starts_with("ssh://git@") {
        bail!("only github.com SSH repo URLs are supported: {raw_url}");
    } else if url.starts_with("https://") || url.starts_with("http://") {
        bail!("only github.com HTTPS repo URLs are supported: {raw_url}");
    } else {
        bail!("unsupported repo URL format: {raw_url}");
    };

    let (org, repo) = path
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("repo URL must identify owner/repo: {raw_url}"))?;

    ensure!(
        !repo.contains('/'),
        "repo URL must identify exactly one GitHub repo: {raw_url}"
    );
    ensure!(
        is_valid_slug(org),
        "invalid GitHub owner in repo URL: {org}"
    );
    ensure!(
        is_valid_slug(repo),
        "invalid GitHub repo in repo URL: {repo}"
    );

    Ok(RepoSpec {
        org: org.to_owned(),
        repo: repo.to_owned(),
        clone_url: format!("git@github.com:{org}/{repo}.git"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_repo_shorthand() {
        let spec = resolve_repo_input("cantina-forks", Some("protocol"), None).unwrap();
        assert_eq!(spec.org, "cantina-forks");
        assert_eq!(spec.repo, "protocol");
        assert_eq!(spec.clone_url, "git@github.com:cantina-forks/protocol.git");
    }

    #[test]
    fn parses_supported_github_urls() {
        for url in [
            "https://github.com/cantina-forks/protocol",
            "https://github.com/cantina-forks/protocol.git",
            "https://github.com/cantina-forks/protocol.git?tab=readme",
            "git@github.com:cantina-forks/protocol.git",
            "ssh://git@github.com/cantina-forks/protocol.git",
        ] {
            let spec = parse_github_repo_url(url).unwrap();
            assert_eq!(spec.org, "cantina-forks");
            assert_eq!(spec.repo, "protocol");
            assert_eq!(spec.clone_url, "git@github.com:cantina-forks/protocol.git");
        }
    }

    #[test]
    fn rejects_nested_or_non_github_urls() {
        assert!(parse_github_repo_url("https://gitlab.com/a/b").is_err());
        assert!(parse_github_repo_url("https://github.com/a/b/c").is_err());
        assert!(parse_github_repo_url("not-a-url").is_err());
    }
}
