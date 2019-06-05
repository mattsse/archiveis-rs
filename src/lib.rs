//! # archiveis - Rust API Wrapper for Archive.is
//! This crate provides simple access to the Archive.is Capturing Service.
//! ## Quick Start
//! ### Creating a Client
//! To create a client to access the Archive.is Capturing Service, you should use the `ArchiveClient`
//! struct. You can pass a specific user agent or none to use a default one.
//! To capture a specific url all you need to do is call the `capture` function of the client provided
//! with the desired url.
//!
//! ### Archive a url
//! The `ArchiveClient` is build with `hyper` and therefor uses futures for its services.
//!
//! ```rust,no_run
//! extern crate archiveis;
//! extern crate futures;
//!
//! use archiveis::ArchiveClient;
//! use futures::future::Future;
//!
//! let client = ArchiveClient::new(Some("archiveis (https://github.com/MattsSe/archiveis-rs)"));
//! let url = "http://example.com/";
//! let capture = client.capture(url).and_then(|archived| {
//!     println!("targeted url: {}", archived.target_url);
//!     println!("url of archived site: {}", archived.archived_url);
//!     println!("archive.is submit token: {}", archived.submit_token);
//!     Ok(())
//! });
//! ```
//! ### Archive multiple urls
//! archive.is uses a temporary token to validate a archive request.
//! The `ArchiveClient` `capture` function first obtains a new submit token via a GET request.
//! The token is usually valid several minutes, and even if archive.is switches to a new in the
//! meantime token,the older ones are still valid. So if we need to archive multiple links,
//! we can only need to obtain the token once and then invoke the capturing service directly with
//! `capture_with_token` for each url. `capture_all` returns a Vec of Results of every capturing
//! request, so every single capture request gets executed regardless of the success of prior requests.
//!
//! ```rust,no_run
//! extern crate archiveis;
//! extern crate futures;
//!
//! use archiveis::ArchiveClient;
//! use futures::future::{join_all, Future};
//!
//! let client = ArchiveClient::new(Some("archiveis (https://github.com/MattsSe/archiveis-rs)"));
//!
//! // the urls to capture
//! let urls = vec![
//!     "http://example.com/",
//!     "https://github.com/MattsSe/archiveis-rs",
//!     "https://crates.io",
//! ];
//!
//! let capture = client.capture_all(urls, None).and_then(|archives| {
//!         let failures: Vec<_> = archives
//!             .iter()
//!             .map(Result::as_ref)
//!             .filter(Result::is_err)
//!             .map(Result::unwrap_err)
//!             .collect();
//!         if failures.is_empty() {
//!             println!("all links successfully archived.");
//!         } else {
//!            for err in failures {
//!                 if let archiveis::Error::MissingUrl(url) = err {
//!                     println!("Failed to archive url: {}", url);
//!                 }
//!             }
//!         }
//!        Ok(())
//!    });
//! ```
//!

//#![deny(warnings)]

use chrono::DateTime;
use futures::future;
use hyper::rt::{Future, Stream};
use hyper::Client;
use hyper::Request;

/// The Error Type used in this crate
///
#[derive(Debug)]
pub enum Error {
    /// Represents an error originated from hyper
    Hyper(hyper::Error),
    /// Means that no token could be obtained from archive.is
    MissingToken,
    /// Means that the POST was successful but no archive url to the requested
    /// url, which `MissingUrl` stores, could be obtained from the HTTP response
    MissingUrl(String),
}

/// Represents a result of the capture service
#[derive(Debug, Clone)]
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
    client: Client<hyper::client::HttpConnector, hyper::Body>,
    /// The user agent used for the HTTP Requests
    user_agent: String,
}

impl ArchiveClient {
    /// Creates a new instance of the `ArchiveClient` using a special user agent
    pub fn new<T: Into<String>>(user_agent: T) -> Self {
        ArchiveClient {
            client: Client::new(),
            user_agent: user_agent.into(),
        }
    }

    /// Invokes the archive.is capture service an each url supplied.
    /// If no token was passed, a fresh token is obtained via `get_unique_token`,
    /// afterwards all capture requests are joined in a single future that returns
    /// a `Vec<Result<Archived, Error>>` which holds every result of the individual
    /// capturing requests, so every single capture request gets executed regardless
    /// of the success of prior requests.
    pub fn capture_all<'a>(
        &'a self,
        urls: Vec<&'a str>,
        token: Option<String>,
    ) -> impl Future<Item = Vec<Result<Archived, Error>>, Error = Error> + 'a {
        use futures::future::join_all;
        let get_token: Box<dyn Future<Item = String, Error = Error>> = match token {
            Some(t) => Box::new(future::ok(t)),
            _ => Box::new(self.get_unique_token()),
        };
        get_token.and_then(move |token| {
            let mut futures = Vec::new();
            for url in urls {
                futures.push(self.capture_with_token(url, &token).then(Ok));
            }
            join_all(futures)
        })
    }

    /// Invokes the archive.is capture service.
    /// First it get's the current valid unique `submitid` by calling `get_unique_id`.
    /// Then it sends a new POST request to the archive.is submit endpoint with the `url` and the
    /// `submitid` encoded as `x-www-form-urlencoded` in the body.
    /// The link to the archived page is then contained in the `Refresh` header of the Response.
    /// It also tries to parse the timemap from the `Date` header and packs it together with the url
    /// in a new `Archived` instance.
    pub fn capture<'a>(&'a self, url: &str) -> impl Future<Item = Archived, Error = Error> + 'a {
        // TODO add lifetime constraints to url instead?
        let u = url.to_string();
        // TODO The id is usually valid a couple minutes, perhaps caching it instead?
        self.get_unique_token()
            .and_then(move |id| self.capture_with_token(&u, id.as_str()))
    }

    /// Invokes the archive.is capture service directly without retrieving a submit id first.
    /// This can have the advantage that no additional request is necessary, but poses potential
    /// drawbacks when the `id` is not valid. Generally the temporarily ``` are still valid
    /// even when the archiv.is server switched to a new one in the meantime. But it might be the
    /// case, that the server returns a `Server Error`, In that case a `Error::MissingUrl(url)` is
    /// returned containing the requested url.
    /// Switching to the ordinary `capture` method would also be possible but that could result in
    /// undesired cyclic behavior.
    /// There might also be the possibility, where the response body already
    /// contains the html of the archived `url`. In that case we read the archive.is url from the
    /// html's meta information instead.
    pub fn capture_with_token<'a>(
        &'a self,
        url: &str,
        submit_token: &str,
    ) -> impl Future<Item = Archived, Error = Error> + 'a {
        use chrono::TimeZone;

        let target_url = url.to_owned();
        let body: String = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("url", &target_url)
            .append_pair("anyway", "1")
            .append_pair("submitid", submit_token)
            .finish();
        let submit_token = submit_token.to_owned();
        // prepare the POST request
        let req = Request::post("http://archive.is/submit/")
            .header("User-Agent", self.user_agent.as_str())
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body.into())
            .unwrap();
        let capture = self
            .client
            .request(req)
            .map_err(Error::Hyper)
            .and_then(move |resp| {
                // get the url of the archived page
                let refresh = resp.headers().get("Refresh").and_then(|x| {
                    x.to_str()
                        .ok()
                        .and_then(|x| x.split('=').nth(1).map(str::to_owned))
                });
                let archived: Box<dyn Future<Item = Archived, Error = Error>> = match refresh {
                    Some(archived_url) => {
                        // parse the timemap from the Date header
                        let time_stamp = resp.headers().get("Date").and_then(|x| {
                            x.to_str().ok().and_then(|x| {
                                chrono::Utc.datetime_from_str(x, "%a, %e %b %Y %T GMT").ok()
                            })
                        });
                        let archived = Archived {
                            target_url,
                            archived_url,
                            time_stamp,
                            submit_token,
                        };
                        Box::new(future::ok(archived))
                    }
                    _ => {
                        // an err response body can be empty, contain Server Error or
                        // can directly contain the archived site, in that case we extract the archived_url
                        let err_resp_handling = resp
                            .into_body()
                            .concat2()
                            .map_err(Error::Hyper)
                            .and_then(move |ch| {
                                if let Ok(html) = ::std::str::from_utf8(&ch) {
                                    if html.starts_with("<h1>Server Error</h1>") {
                                        return Box::new(self.capture(target_url.as_str()))
                                            as Box<dyn Future<Item = Archived, Error = Error>>;
                                    }
                                    let archived_url = html
                                        .splitn(2, "<meta property=\"og:url\"")
                                        .nth(1)
                                        .and_then(|x| {
                                            x.splitn(2, "content=\"").nth(1).and_then(|id| {
                                                id.splitn(2, '\"').next().map(str::to_owned)
                                            })
                                        });
                                    if let Some(archived_url) = archived_url {
                                        let archived = Archived {
                                            target_url,
                                            archived_url,
                                            time_stamp: None,
                                            submit_token,
                                        };
                                        return Box::new(future::ok(archived));
                                    }
                                }
                                // TODO possible cycle: calling self.capture can cause an undesired loop
                                // Box::new(self.capture(target_url.as_str()))
                                // return an Error instead
                                Box::new(future::err(Error::MissingUrl(target_url)))
                            });
                        Box::new(err_resp_handling)
                    }
                };
                archived
            });
        Box::new(capture)
    }

    /// In order to submit an authorized capture request we need to first obtain a temporarily valid
    /// unique token.
    /// This is achieved by sending a GET request to the archive.is domain and parsing the `
    /// `submitid` from the responding html.
    pub fn get_unique_token(&self) -> impl Future<Item = String, Error = Error> {
        let req = Request::get("http://archive.is/")
            .header("User-Agent", self.user_agent.as_str())
            .body(hyper::Body::empty())
            .unwrap();

        self.client
            .request(req)
            .map_err(Error::Hyper)
            .and_then(|res| {
                res.into_body()
                    .concat2()
                    .map_err(Error::Hyper)
                    .and_then(|ch| {
                        ::std::str::from_utf8(&ch)
                            .map_err(|_| Error::MissingToken)
                            .and_then(|html| {
                                html.rsplitn(2, "name=\"submitid")
                                    .next()
                                    .and_then(|x| {
                                        x.splitn(2, "value=\"").nth(1).and_then(|token| {
                                            token.splitn(2, '\"').next().map(str::to_owned)
                                        })
                                    })
                                    .ok_or(Error::MissingToken)
                            })
                    })
            })
    }
}

impl Default for ArchiveClient {
    fn default() -> Self {
        ArchiveClient {
            client: Client::new(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_13_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/67.0.3396.99 Safari/537.36".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn extract_unique_token() {
        let html = r###"type="hidden" name="submitid" value="1yPA39C6QcM84Dzspl+7s28rrAFOnliPMCiJtoP+OlTKmd5kJd21G4ucgTkx0mnZ"/>"###;

        let split = html.rsplitn(2, "name=\"submitid").next().and_then(|x| {
            x.splitn(2, "value=\"")
                .nth(1)
                .and_then(|id| id.splitn(2, "\"").next().map(|x| x.to_owned()))
        });
        assert_eq!(
            Some("1yPA39C6QcM84Dzspl+7s28rrAFOnliPMCiJtoP+OlTKmd5kJd21G4ucgTkx0mnZ".to_owned()),
            split
        );
    }
}
