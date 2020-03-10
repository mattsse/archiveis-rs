//! # archiveis - Rust API Wrapper for Archive.is
//! This crate provides simple access to the Archive.is Capturing Service.
//! ## Quick Start
//!
//! ### Archive a url
//! The `ArchiveClient` is build with `hyper` and uses futures for capturing archive.is links.
//!
//! ```no_run
//! # use archiveis::ArchiveClient;
//! # use tokio::prelude::*;
//! #
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = ArchiveClient::default();
//! let archived = client.capture("http://example.com/").await?;
//! println!("targeted url: {}", archived.target_url);
//! println!("url of archived site: {}", archived.archived_url);
//! println!("archive.is submit token: {}", archived.submit_token);
//! # Ok(())
//! # }
//! ```
//!
//! ### Archive multiple urls
//! archive.is uses a temporary token to validate a archive request.
//! The `ArchiveClient` `capture` function first obtains a new submit token via a GET request.
//! The token is usually valid several minutes, and even if archive.is switched to a new in the
//! meantime token,the older ones are still valid. So if we need to archive multiple links,
//! we can only need to obtain the token once and then invoke the capturing service directly with
//! `capture_with_token` for each url. `capture_all` returns a Vec of Results of every capturing
//! request, so every single capture request gets executed regardless of the success of prior requests.
//!
//! ```no_run
//! # use archiveis::ArchiveClient;
//! # use tokio::prelude::*;
//! #
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = ArchiveClient::default();
//!
//! // the urls to capture
//! let urls = vec![
//!     "http://example.com/",
//!     "https://github.com/MattsSe/archiveis-rs",
//!     "https://crates.io",
//! ];
//!
//! let (archived, failures) : (Vec<_>, Vec<_>) = client.capture_all(urls).await?.into_iter()
//!             .partition(Result::is_ok);
//!
//! let archived: Vec<_> = archived.into_iter().map(Result::unwrap).collect();
//! let failures: Vec<_> = failures.into_iter().map(Result::unwrap_err).collect();
//! if failures.is_empty() {
//!     println!("all links successfully archived.");
//! } else {
//!     for err in &failures {
//!         if let archiveis::Error::MissingUrl(url) | archiveis::Error::ServerError(url) = err {
//!             println!("Failed to archive url: {}", url);
//!         }
//!     }
//! }
//! #   Ok(())
//! # }
//! ```
//!

//#![deny(warnings)]
#[macro_use]
extern crate log;

#[cfg(feature = "with-serde")]
use serde::{Deserialize, Serialize};

use chrono::offset::TimeZone;
use chrono::DateTime;
use futures::{stream, StreamExt};
use reqwest::{header, IntoUrl};
use std::fmt;
use std::rc::Rc;

/// The Error Type used in this crate
#[derive(Debug)]
pub enum Error {
    /// Represents an error originated from hyper
    Reqwest(reqwest::Error),
    /// Means that no token could be obtained from archive.is
    MissingToken,
    /// Means that the POST was successful but no archive url to the requested
    /// url, which `MissingUrl` stores, could be obtained from the HTTP response
    MissingUrl(String),
    /// An error occurred on the archiveis server while archiving an url
    ServerError(String),
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error::Reqwest(err)
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::MissingToken => write!(f, "Missing required token."),
            Error::Reqwest(err) => err.fmt(f),
            Error::MissingUrl(url) => write!(f, "Missing archiveis url after archiving {}", url),
            Error::ServerError(url) => write!(f, "Encountered server error for {}", url),
        }
    }
}

/// Result type for this crate
pub type Result<T> = ::std::result::Result<T, Error>;

/// Represents a result of the capture service
#[derive(Debug, Clone)]
#[cfg_attr(feature = "with-serde", derive(Serialize, Deserialize))]
pub struct Archived {
    /// The requested url to archive with the archive.is capture service
    pub target_url: String,
    /// The archive.is url that archives the `target_url`
    pub archived_url: String,
    /// The time stamp when the site was archived
    pub time_stamp: Option<DateTime<chrono::Utc>>,
    /// The submitid token used to authorize access on the archive.is server
    pub submit_token: String,
}

/// A Client that serves as a wrapper around the archive.is capture service
pub struct ArchiveClient {
    /// The internal Hyper Http Client.
    client: Rc<reqwest::Client>,
}

impl ArchiveClient {
    /// Creates a new instance of the `ArchiveClient` using a special user agent
    pub fn new<T: ToString>(user_agent: T) -> Self {
        let mut headers = header::HeaderMap::with_capacity(1);
        headers.insert(
            header::USER_AGENT,
            user_agent
                .to_string()
                .parse()
                .expect("Failed to parse user agent."),
        );
        let client = reqwest::ClientBuilder::default()
            .default_headers(headers)
            .build()
            .expect("Failed to create reqwest client");

        ArchiveClient {
            client: Rc::new(client),
        }
    }

    /// Invokes the archive.is capture service on each url provided.
    ///
    /// If no token was passed, a fresh token is obtained via `get_unique_token`,
    /// afterwards all capture requests are joined in a single future that returns
    /// a `Vec<Result<Archived, Error>>` which holds every result of the individual
    /// capturing requests, so every single capture request gets executed regardless
    /// of the success of prior requests.
    pub async fn capture_all<U: IntoUrl>(self, links: Vec<U>) -> Result<Vec<Result<Archived>>> {
        let token = self.get_unique_token().await?;

        Ok(stream::iter(
            links
                .into_iter()
                .map(|url| async { self.capture_with_token(url, token.clone()).await }),
        )
        .buffer_unordered(10)
        .collect::<Vec<_>>()
        .await)
    }

    /// Invokes the archive.is capture service.
    /// First it get's the current valid unique `submitid` by calling `get_unique_id`.
    /// Then it sends a new POST request to the archive.is submit endpoint with the `url` and the
    /// `submitid` encoded as `x-www-form-urlencoded` in the body.
    /// The link to the archived page is then contained in the `Refresh` header of the Response.
    /// It also tries to parse the timemap from the `Date` header and packs it together with the url
    /// in a new `Archived` instance.
    pub async fn capture<U: IntoUrl>(&self, url: U) -> Result<Archived> {
        self.capture_with_token(url, self.get_unique_token().await?)
            .await
    }

    /// Invokes the archive.is capture service directly without retrieving a submit id first.
    /// This can have the advantage that no additional request is necessary, but poses potential
    /// drawbacks when the `id` is not valid. In general the temporarily tokens are still valid
    /// even when the archiv.is server switched to a new one in the meantime. But it might be the
    /// case, that the server returns a `Server Error`, In that case a `Error::ServerError(url)` is
    /// returned containing the requested url.
    /// Switching to the ordinary `capture` method would also be possible but that could result in
    /// undesired cyclic behavior.
    /// There might also be the possibility, where the response body already
    /// contains the html of the archived `url`. In that case we read the archive.is url from the
    /// html's meta information instead.
    pub async fn capture_with_token<U: IntoUrl, T: ToString>(
        &self,
        url: U,
        submit_token: T,
    ) -> Result<Archived> {
        let target_url = url.into_url()?;
        let submit_token = submit_token.to_string();
        let body: String = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("url", target_url.as_str())
            .append_pair("anyway", "1")
            .append_pair("submitid", &submit_token)
            .finish();

        let resp = self
            .client
            .post(target_url.clone())
            .body(body)
            .send()
            .await?;

        if let Some(archived_url) = resp.headers().get("Refresh").and_then(|x| {
            x.to_str()
                .ok()
                .and_then(|x| x.split('=').nth(1).map(str::to_string))
        }) {
            // parse the timemap from the Date header
            let time_stamp = resp.headers().get("Date").and_then(|x| {
                x.to_str()
                    .ok()
                    .and_then(|x| chrono::Utc.datetime_from_str(x, "%a, %e %b %Y %T GMT").ok())
            });
            let archived = Archived {
                target_url: target_url.to_string(),
                archived_url,
                time_stamp,
                submit_token: submit_token.to_string(),
            };
            debug!(
                "Archived target url {} at {}",
                archived.target_url, archived.archived_url
            );

            return Ok(archived);
        } else {
            // an err response body can be empty, contain Server Error or
            // can directly contain the archived site, in that case we extract the archived_url

            if let Ok(html) = resp.text().await {
                if html.starts_with("<h1>Server Error</h1>") {
                    error!("Server Error while archiving {}", target_url);

                    return Err(Error::ServerError(target_url.to_string()));
                }
                let archived_url =
                    html.splitn(2, "<meta property=\"og:url\"")
                        .nth(1)
                        .and_then(|x| {
                            x.splitn(2, "content=\"")
                                .nth(1)
                                .and_then(|id| id.splitn(2, '\"').next().map(str::to_owned))
                        });
                if let Some(archived_url) = archived_url {
                    let archived = Archived {
                        target_url: target_url.to_string(),
                        archived_url,
                        time_stamp: None,
                        submit_token: submit_token.to_string(),
                    };
                    debug!(
                        "Archived target url {} at {}",
                        archived.target_url, archived.archived_url
                    );
                    return Ok(archived);
                }
            }
            error!("Failed to archive {}", target_url);
            return Err(Error::MissingUrl(target_url.into_string()));
        }
    }

    /// In order to submit an authorized capture request we need to first obtain a temporarily valid
    /// unique token.
    ///
    /// This is achieved by sending a GET request to the archive.is domain and parsing the `
    /// `submitid` from the responding html.
    pub async fn get_unique_token(&self) -> Result<String> {
        let html = self
            .client
            .get("http://archive.is/")
            .send()
            .await?
            .text()
            .await
            .map_err(|_| Error::MissingToken)?;

        html.rsplitn(2, "name=\"submitid")
            .next()
            .and_then(|x| {
                x.splitn(2, "value=\"")
                    .nth(1)
                    .and_then(|token| token.splitn(2, '\"').next().map(str::to_string))
            })
            .ok_or(Error::MissingToken)
    }
}

impl Default for ArchiveClient {
    fn default() -> Self {
        ArchiveClient::new("archiveis-rs")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn extract_unique_token() {
        let html = r###"type="hidden" name="submitid" value="1yPA39C6QcM84Dzspl+7s28rrAFOnliPMCiJtoP+OlTKmd5kJd21G4ucgTkx0mnZ"/>"###;

        let split = html
            .rsplitn(2, "name=\"submitid")
            .filter_map(|x| {
                x.splitn(2, "value=\"")
                    .skip(1)
                    .filter_map(|token| token.splitn(2, '\"').next())
                    .next()
            })
            .next();
        assert_eq!(
            Some("1yPA39C6QcM84Dzspl+7s28rrAFOnliPMCiJtoP+OlTKmd5kJd21G4ucgTkx0mnZ"),
            split
        );
    }
}
