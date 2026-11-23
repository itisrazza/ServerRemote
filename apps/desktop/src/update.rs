//! Checking GitHub Releases for a newer version of the app.
//!
//! Only reads the latest release's version and page; getting the update is
//! left to the browser.

use std::fmt;

use semver::Version;
use serde::Deserialize;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROJECT_URL: &str = env!("CARGO_PKG_REPOSITORY");

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/itisrazza/ServerRemote/releases/latest";

/// Where an update check stands, for the About section.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    NotChecked,
    Checking,
    Checked(Result<Outcome, Error>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    Available {
        version: Version,
        url: String,
    },
    /// Nothing has been published, or the repository isn't public.
    NoReleases,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The request couldn't be made or completed.
    Request(String),
    /// GitHub answered with an unexpected status.
    Status { code: u16, body: String },
    /// The answer didn't contain a usable release.
    Parse(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Request(message) => write!(f, "request failed: {message}"),
            Error::Status { code, body } => write!(f, "GitHub answered {code}: {body}"),
            Error::Parse(message) => write!(f, "unexpected answer: {message}"),
        }
    }
}

/// The fields of a GitHub release the check needs.
#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
}

/// Asks GitHub for the latest release and compares it with this build.
pub async fn check() -> Result<Outcome, Error> {
    tokio::task::spawn_blocking(fetch_latest)
        .await
        .map_err(|error| Error::Request(error.to_string()))?
}

fn fetch_latest() -> Result<Outcome, Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .get(LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .header(
            "User-Agent",
            concat!("ServerRemote/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|error| Error::Request(error.to_string()))?;

    let status = response.status();
    // GitHub answers 404 both when there's no release yet and when the
    // repository isn't visible to anonymous requests.
    if status == 404 {
        return Ok(Outcome::NoReleases);
    }
    if !status.is_success() {
        let body = response
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|error| format!("(body unreadable: {error})"));
        return Err(Error::Status {
            code: status.as_u16(),
            body,
        });
    }

    let release: Release = response
        .body_mut()
        .read_json()
        .map_err(|error| Error::Parse(error.to_string()))?;
    compare(VERSION, &release)
}

fn compare(current: &str, release: &Release) -> Result<Outcome, Error> {
    let current = Version::parse(current)
        .map_err(|error| Error::Parse(format!("this build's version {current:?}: {error}")))?;
    let tag = release.tag_name.trim_start_matches('v');
    let latest = Version::parse(tag).map_err(|error| {
        Error::Parse(format!(
            "release tag {:?} isn't a version: {error}",
            release.tag_name
        ))
    })?;

    Ok(if latest > current {
        Outcome::Available {
            version: latest,
            url: release.html_url.clone(),
        }
    } else {
        Outcome::UpToDate
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> Release {
        Release {
            tag_name: tag.to_owned(),
            html_url: "https://example.invalid/release".to_owned(),
        }
    }

    #[test]
    fn comparing_with_the_latest_release() {
        let available = |version: &str| {
            Ok(Outcome::Available {
                version: Version::parse(version).unwrap(),
                url: "https://example.invalid/release".to_owned(),
            })
        };
        let cases = [
            ("newer release", "0.1.0", "v0.2.0", available("0.2.0")),
            ("tag without a v", "0.1.0", "0.1.1", available("0.1.1")),
            ("same version", "0.2.0", "v0.2.0", Ok(Outcome::UpToDate)),
            ("older release", "0.3.0", "v0.2.0", Ok(Outcome::UpToDate)),
            (
                "release beats its pre-release",
                "1.0.0-beta.1",
                "v1.0.0",
                available("1.0.0"),
            ),
        ];
        for (name, current, tag, expected) in cases {
            assert_eq!(compare(current, &release(tag)), expected, "{name}");
        }
    }

    #[test]
    fn a_tag_that_isnt_a_version_is_an_error() {
        assert!(matches!(
            compare("0.1.0", &release("nightly")),
            Err(Error::Parse(_))
        ));
    }

    #[test]
    fn this_builds_version_parses() {
        assert!(Version::parse(VERSION).is_ok());
    }
}
