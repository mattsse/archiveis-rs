#![allow(dead_code)]
#![allow(unused_variables)]
extern crate futures;
extern crate hyper;

use futures::future;
use hyper::header::HeaderValue;
use hyper::rt::{Future, Stream};
use hyper::Client;
use hyper::{HeaderMap, Request};

pub enum Error {
    Hyper(hyper::Error),
    Uri,
}

const DOMAIN: &'static str = "http://archive.is/";

pub struct ArchiveClient {
    client: Client<hyper::client::HttpConnector, hyper::Body>,
    user_agent: String,
}

impl ArchiveClient {
    fn default_headers(
        headers: &mut HeaderMap<hyper::header::HeaderValue>,
        user_agent: &str,
    ) -> Result<(), hyper::header::InvalidHeaderValue> {
        headers.insert("User-Agent", HeaderValue::from_str(user_agent)?);
        Ok(())
    }

    pub fn new(user_agent: Option<String>) -> Self {
        ArchiveClient {
            client: Client::new(),
            user_agent: user_agent.unwrap_or("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_13_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/67.0.3396.99 Safari/537.36".to_owned()),
        }
    }
    pub fn save(self, url: &str) -> Result<(), hyper::Error> {
        //        let uri: hyper::Uri = url.parse().map_err(|_| Error::Uri)?;

        // get an unique id first

        let body = format!("{{\"url\": {:?} }}", url);
        let req = Request::post("https://archive.is/submit/")
            .header("User-Agent", self.user_agent.as_str())
            .body(body.into())
            .unwrap();

        self.client.request(req).and_then(|req| Ok(()));

        Ok(())
    }

    pub fn get_unique_id(self) -> impl Future<Item = Option<String>, Error = hyper::Error> {
        let req = Request::get(DOMAIN)
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
                                .skip(1)
                                .next()
                                .and_then(|id| id.splitn(2, "\"").next().map(|x| x.to_owned()))
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
            x.rsplitn(2, "value=\"")
                .next()
                .and_then(|id| id.splitn(2, "\"").next().map(|x| x.to_owned()))
        });

        assert_eq!(
            Some("1yPA39C6QcM84Dzspl+7s28rrAFOnliPMCiJtoP+OlTKmd5kJd21G4ucgTkx0mnZ".to_owned()),
            split
        );
    }
}
