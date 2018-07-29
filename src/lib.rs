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
//!     println!("url of archived site: {}", archived.unwrap().url);
//!     Ok(())
//! });
//!

#![deny(warnings)]
extern crate chrono;
extern crate futures;
extern crate hyper;
extern crate url;

use chrono::DateTime;
use futures::future;
use hyper::rt::{Future, Stream};
use hyper::Client;
use hyper::Request;

/// Represents a result of the capture service
#[derive(Debug, Clone)]
pub struct Archived {
    /// The url to the archived site
    pub url: String,
    /// The time stamp when the site was archived
    pub time_stamp: Option<DateTime<chrono::Utc>>,
}

/// A Client that serves as a wrapper around the archive.is capture service
pub struct ArchiveClient {
    /// The internal Hyper Http Client.
    client: Client<hyper::client::HttpConnector, hyper::Body>,
    /// The user agent used for the HTTP Requests
    user_agent: String,
}

impl ArchiveClient {
    /// Creates a new instance of the `ArchiveClient` using the provided user agent or a dummy one.
    pub fn new(user_agent: Option<&str>) -> Self {
        ArchiveClient {
            client: Client::new(),
            user_agent: user_agent.map(|x| x.to_owned()).unwrap_or_else(|| {
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_13_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/67.0.3396.99 Safari/537.36".to_owned()
            }),
        }
    }

    /// invokes the archive.is capture service
    /// First it get's the current valid unique `submitid` by calling `get_unique_id`.
    /// Then it sends a new POST request to the archive.is submit endpoint with the `url` and the
    /// `submitid` encoded as `x-www-form-urlencoded` in the body.
    /// The link to the archived page is then contained in the `Refresh` header of the Response.
    /// It also tries to parse the timemap from the `Date` header and packs it together with the url
    /// in a new `Archived` instance.
    pub fn capture<'a>(
        &'a self,
        url: &str,
    ) -> impl Future<Item = Option<Archived>, Error = hyper::Error> + 'a {
        use chrono::TimeZone;
        use url::form_urlencoded;
        // TODO add lifetime constraints to url instead?
        let u = url.to_owned();
        // TODO The id is usually valid a couple minutes, perhaps caching it instead?
        self.get_unique_id().and_then(move |resp| {
            let res: Box<Future<Item = Option<Archived>, Error = hyper::Error>> = match resp {
                Some(id) => {
                    // encode the data for the post body
                    let body: String = form_urlencoded::Serializer::new(String::new())
                        .append_pair("url", u.as_str())
                        .append_pair("anyway", "1")
                        .append_pair("submitid", id.as_str())
                        .finish();
                    // prepare the POST request
                    let req = Request::post("http://archive.is/submit/")
                        .header("User-Agent", self.user_agent.as_str())
                        .header("Content-Type", "application/x-www-form-urlencoded")
                        .body(body.into())
                        .unwrap();
                    let capture = self.client.request(req).and_then(|resp| {
                        // get the url of the archived page
                        let refresh = resp.headers().get("Refresh").and_then(|x| {
                            x.to_str()
                                .ok()
                                .and_then(|x| x.split('=').nth(1).map(str::to_owned))
                        });
                        if let Some(tiny_url) = refresh {
                            // parse the timemap from the Date header
                            let time_stamp = resp.headers().get("Date").and_then(|x| {
                                x.to_str().ok().and_then(|x| {
                                    chrono::Utc.datetime_from_str(x, "%a, %e %b %Y %T GMT").ok()
                                })
                            });
                            let archived = Archived {
                                url: tiny_url,
                                time_stamp,
                            };
                            Ok(Some(archived))
                        } else {
                            Ok(None)
                        }
                    });
                    Box::new(capture)
                }
                _ => Box::new(future::ok(None)),
            };
            res
        })
    }

    /// In order to submit an authorized capture request we need to first obtain a temporarily valid
    /// unique identifier, or none could be found.
    /// This is achieved by sending a GET request to the archive.is domain and parsing the `
    /// `submitid` from the responding html.
    pub fn get_unique_id(&self) -> impl Future<Item = Option<String>, Error = hyper::Error> {
        let req = Request::get("http://archive.is/")
            .header("User-Agent", self.user_agent.as_str())
            .body(hyper::Body::empty())
            .unwrap();

        self.client
            .request(req)
            .and_then(|res| {
                res.into_body().concat2().map(|ch| {
                    ::std::str::from_utf8(&ch).and_then(|html| {
                        Ok(html.rsplitn(2, "name=\"submitid").next().and_then(|x| {
                            x.splitn(2, "value=\"")
                                .nth(1)
                                .and_then(|id| id.splitn(2, '\"').next().map(str::to_owned))
                        }))
                    })
                })
            })
            .and_then(|x| Ok(x.unwrap_or(None)))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn extract_unique_id() {
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
