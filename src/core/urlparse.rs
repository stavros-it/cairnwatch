//! Repo URL parsing: turn a pasted URL into (host, owner, name).
//!
//! Accepted:
//! - `github.com/owner/repo` with/without `https://`, `www.`, trailing `/`,
//!   trailing `.git`
//! - `gitlab.com/owner/repo`, nested `gitlab.com/grp/sub/repo` (owner is the
//!   full group path), self-hosted `https://gitlab.example.com/owner/repo`
//!
//! Rejected: empty input, hosts with no path, non-repo hosts
//! (`example.com`), unsupported forges (`bitbucket.org`).

use super::models::RepoHost;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Not recognizable as a repo URL at all.
    NotARepoUrl,
    /// A known forge we do not support (name the host so the UI can say so).
    UnsupportedHost(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NotARepoUrl => write!(f, "not a GitHub/GitLab repo URL"),
            ParseError::UnsupportedHost(h) => {
                write!(f, "unsupported host: {h} (only GitHub and GitLab)")
            }
        }
    }
}

/// Normalize: lowercase, strip scheme, `www.`, trailing `/` and `.git`.
/// Returns (host, path-segments).
fn split_host_path(input: &str) -> Option<(String, Vec<String>)> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    let s = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
    let s = s.strip_prefix("www.").unwrap_or(s);
    // First segment is the host; the rest is the path.
    let mut parts = s.split('/');
    let host = parts.next()?.to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let segments: Vec<String> = parts
        .filter(|p| !p.is_empty())
        .map(|p| {
            let p = p.strip_suffix(".git").unwrap_or(p);
            p.to_string()
        })
        .collect();
    Some((host, segments))
}

/// Parse a pasted repo URL into (host, owner, name).
pub fn parse_repo_url(input: &str) -> Result<(RepoHost, String, String), ParseError> {
    let (host, segments) = split_host_path(input).ok_or(ParseError::NotARepoUrl)?;

    if host == "github.com" {
        if segments.len() != 2 {
            return Err(ParseError::NotARepoUrl);
        }
        return Ok((RepoHost::GitHub, segments[0].clone(), segments[1].clone()));
    }

    // gitlab.com and self-hosted gitlab.* instances.
    if host == "gitlab.com" || host.starts_with("gitlab.") {
        if segments.len() < 2 {
            return Err(ParseError::NotARepoUrl);
        }
        let name = segments.last().unwrap().clone();
        let owner = segments[..segments.len() - 1].join("/");
        return Ok((RepoHost::GitLab { host }, owner, name));
    }

    // Known unsupported forges get a specific message; anything else is
    // simply not a repo URL we understand.
    const UNSUPPORTED: &[&str] = &["bitbucket.org", "sourceforge.net", "codeberg.org"];
    if UNSUPPORTED.contains(&host.as_str()) {
        return Err(ParseError::UnsupportedHost(host));
    }
    Err(ParseError::NotARepoUrl)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh(owner: &str, name: &str) -> Result<(RepoHost, String, String), ParseError> {
        Ok((
            RepoHost::GitHub,
            owner.to_string(),
            name.to_string(),
        ))
    }

    #[test]
    fn github_forms_parse() {
        assert_eq!(parse_repo_url("https://github.com/rust-lang/rust"), gh("rust-lang", "rust"));
        assert_eq!(parse_repo_url("github.com/rust-lang/rust"), gh("rust-lang", "rust"));
        assert_eq!(parse_repo_url("https://www.github.com/rust-lang/rust/"), gh("rust-lang", "rust"));
        assert_eq!(parse_repo_url("github.com/rust-lang/rust.git"), gh("rust-lang", "rust"));
        assert_eq!(parse_repo_url("https://github.com/rust-lang/rust.git/"), gh("rust-lang", "rust"));
    }

    #[test]
    fn gitlab_com_parses() {
        let (host, owner, name) =
            parse_repo_url("https://gitlab.com/gitlab-org/gitlab-runner").unwrap();
        assert_eq!(host, RepoHost::GitLab { host: "gitlab.com".into() });
        assert_eq!(owner, "gitlab-org");
        assert_eq!(name, "gitlab-runner");
    }

    #[test]
    fn gitlab_nested_groups_owner_is_full_group_path() {
        let (host, owner, name) =
            parse_repo_url("gitlab.com/grp/sub/deep/repo").unwrap();
        assert_eq!(host, RepoHost::GitLab { host: "gitlab.com".into() });
        assert_eq!(owner, "grp/sub/deep");
        assert_eq!(name, "repo");
    }

    #[test]
    fn self_hosted_gitlab_host_is_preserved() {
        let (host, owner, name) =
            parse_repo_url("https://gitlab.example.com/acme/widget.git").unwrap();
        assert_eq!(host, RepoHost::GitLab { host: "gitlab.example.com".into() });
        assert_eq!(owner, "acme");
        assert_eq!(name, "widget");
    }

    #[test]
    fn not_a_repo_url_cases() {
        assert_eq!(parse_repo_url(""), Err(ParseError::NotARepoUrl));
        assert_eq!(parse_repo_url("   "), Err(ParseError::NotARepoUrl));
        assert_eq!(parse_repo_url("github.com"), Err(ParseError::NotARepoUrl));
        assert_eq!(parse_repo_url("github.com/owner"), Err(ParseError::NotARepoUrl));
        assert_eq!(parse_repo_url("https://example.com/x"), Err(ParseError::NotARepoUrl));
        // GitLab with only one segment is not a repo.
        assert_eq!(parse_repo_url("gitlab.com/gitlab-org"), Err(ParseError::NotARepoUrl));
    }

    #[test]
    fn unsupported_forges_are_named() {
        assert_eq!(
            parse_repo_url("https://bitbucket.org/o/r"),
            Err(ParseError::UnsupportedHost("bitbucket.org".into()))
        );
    }

    #[test]
    fn trailing_git_only_stripped_from_last_segment() {
        // A repo literally named "git" keeps its name; ".git" suffix stripped.
        let (_, owner, name) = parse_repo_url("github.com/o/gits.git").unwrap();
        assert_eq!(owner, "o");
        assert_eq!(name, "gits");
    }
}
