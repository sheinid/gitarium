use anyhow::{Result, bail};
use serde::Serialize;

pub const DEFAULT_BASE_URL: &str = "https://sheinid.github.io/gitarium";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Repository {
    pub owner: String,
    pub name: String,
}
impl Repository {
    pub fn parse(input: &str) -> Result<Self> {
        let raw = input
            .strip_prefix("https://github.com/")
            .unwrap_or(input)
            .trim_end_matches('/');
        let Some((owner, name)) = raw.split_once('/') else {
            bail!("Expected OWNER/REPOSITORY or its https://github.com URL");
        };
        let name = name.strip_suffix(".git").unwrap_or(name);
        if owner.is_empty()
            || owner.len() > 39
            || !owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || owner.starts_with('-')
            || owner.ends_with('-')
        {
            bail!("Invalid GitHub owner");
        }
        if name.is_empty()
            || name.len() > 100
            || matches!(name, "." | "..")
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            bail!("Invalid GitHub repository name");
        }
        Ok(Self {
            owner: owner.to_ascii_lowercase(),
            name: name.to_ascii_lowercase(),
        })
    }
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
    pub fn markdown(&self, base: &str) -> Result<serde_json::Value> {
        let base = base.trim_end_matches('/');
        if !(base.starts_with("https://")
            || base.starts_with("http://localhost:")
            || base.starts_with("http://127.0.0.1:"))
            || base.bytes().any(|b| {
                b.is_ascii_whitespace()
                    || matches!(
                        b,
                        b'"' | b'\'' | b'<' | b'>' | b'[' | b']' | b'(' | b')' | b'#' | b'?'
                    )
            })
        {
            bail!("Embed base URL must be an HTTPS URL without a query or fragment");
        }
        let url = format!("{base}/embed/{}/{}.gif", self.owner, self.name);
        let request_url = format!(
            "https://github.com/sheinid/gitarium/issues/new?template=render.yml&repository={}%2F{}",
            self.owner, self.name
        );
        Ok(serde_json::json!({
            "repository":self.slug(), "url":url, "request_url":request_url,
            "registration_required":true,
            "markdown":format!("![{} growing with Gitarium]({url})",self.slug())
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn github_slugs_cannot_be_urls_paths_or_commands() {
        for input in [
            "../repo",
            "owner/..",
            "owner/repo/extra",
            "https://evil.example/owner/repo",
            "-owner/repo",
            "owner/repo?token=x",
            "owner/repo%2f..",
            "owner/repo\n",
            "owner/repo;command",
        ] {
            assert!(Repository::parse(input).is_err(), "{input}");
        }
        assert_eq!(
            Repository::parse("https://github.com/Sheinid/gitarium.git")
                .unwrap()
                .slug(),
            "sheinid/gitarium"
        );
        assert_eq!(
            Repository::parse("owner/.github").unwrap().slug(),
            "owner/.github"
        );
    }
    #[test]
    fn markdown_includes_the_pages_url_and_first_render_request() {
        let repo = Repository::parse("sheinid/gitarium").unwrap();
        let data = repo.markdown("https://garden.example/").unwrap();
        assert_eq!(
            data["url"],
            "https://garden.example/embed/sheinid/gitarium.gif"
        );
        assert!(data["markdown"].as_str().unwrap().starts_with("!["));
        assert!(
            data["request_url"]
                .as_str()
                .unwrap()
                .ends_with("repository=sheinid%2Fgitarium")
        );
        assert_eq!(data["registration_required"], true);
        assert!(repo.markdown("https://example.com/)[injection]").is_err());
    }
}
