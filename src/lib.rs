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

#[derive(Debug, Clone)]
pub struct Archived {
    pub tiny_url: String,
    pub time_stamp: Option<DateTime<chrono::Utc>>,
}

const DOMAIN: &str = "http://archive.is/";

pub struct ArchiveClient {
    client: Client<hyper::client::HttpConnector, hyper::Body>,
    user_agent: String,
}

impl ArchiveClient {
    pub fn new(user_agent: Option<&str>) -> Self {
        ArchiveClient {
            client: Client::new(),
            user_agent: user_agent.map(|x| x.to_owned()).unwrap_or_else(|| {
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_13_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/67.0.3396.99 Safari/537.36".to_owned()
            }),
        }
    }
    pub fn capture<'a>(
        &'a self,
        url: &str,
    ) -> impl Future<Item = Option<Archived>, Error = hyper::Error> + 'a {
        use chrono::TimeZone;
        use url::form_urlencoded;
        // TODO add liftime constraints to url instead?
        let u = url.to_owned();
        self.get_unique_id().and_then(move |resp| {
            let res: Box<Future<Item = Option<Archived>, Error = hyper::Error>> = match resp {
                Some(id) => {
                    let body: String = form_urlencoded::Serializer::new(String::new())
                        .append_pair("url", u.as_str())
                        .append_pair("anyway", "1")
                        .append_pair("submitid", id.as_str())
                        .finish();
                    let req = Request::post("http://archive.is/submit/")
                        .header("User-Agent", self.user_agent.as_str())
                        .header("Content-Type", "application/x-www-form-urlencoded")
                        .body(body.into())
                        .unwrap();
                    let capture = self.client.request(req).and_then(|resp| {
                        let refresh = resp.headers().get("Refresh").and_then(|x| {
                            x.to_str()
                                .ok()
                                .and_then(|x| x.split('=').nth(1).map(str::to_owned))
                        });
                        if let Some(tiny_url) = refresh {
                            let time_stamp = resp.headers().get("Date").and_then(|x| {
                                x.to_str().ok().and_then(|x| {
                                    chrono::Utc.datetime_from_str(x, "%a, %e %b %Y %T GMT").ok()
                                })
                            });
                            let archived = Archived {
                                tiny_url,
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

    pub fn get_unique_id(&self) -> impl Future<Item = Option<String>, Error = hyper::Error> {
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
