# archiveis-rs

[![Build Status](https://travis-ci.com/MattsSe/archiveis-rs.svg?branch=master)](https://travis-ci.com/MattsSe/archiveis-rs)
[![Released API docs](https://docs.rs/archiveis/badge.svg)](https://docs.rs/archiveis)

Provides simple access to the Archive.is Capturing Service.
Archive any url and get the corresponding archive.is link in return.

## Examples

The `ArchiveClient` is build with `hyper` and therefor uses futures for its services.

### Archive single url

```rust
extern crate archiveis;
extern crate futures;
extern crate tokio_core;

use archiveis::ArchiveClient;
use futures::future::Future;
use tokio_core::reactor::Core;

fn main() {
 let mut core = Core::new().unwrap();

 let client = ArchiveClient::new(Some("archiveis (https://github.com/MattsSe/archiveis-rs)"));
 let url = "http://example.com/";
 let capture = client.capture(url).and_then(|archived| {
    println!("targeted url: {}", archived.target_url);
    println!("url of archived site: {}", archived.archived_url);
    println!("archive.is submit token: {}", archived.submit_token);
    Ok(())
 });

 core.run(capture).unwrap();
}
```

### Archive mutliple urls

archive.is uses a temporary token to validate a archive request.
The `ArchiveClient` `capture` function first obtains a new submit token via a GET request.
The token is usually valid several minutes, and even if archive.is switches to a new in the
meantime token,the older ones are still valid. So if we need to archive multiple links,
we can only need to obtain the token once and then invoke the capturing service directly with
`capture_with_token` for each url. `capture_all` returns a Vec of Results of every capturing
request, so every single capture request gets executed regardless of the success of prior requests.

```rust
extern crate archiveis;
extern crate futures;
extern crate tokio_core;

use archiveis::ArchiveClient;
use futures::future::{join_all, Future};
use tokio_core::reactor::Core;
fn main() {
    let mut core = Core::new().unwrap();

    let client = ArchiveClient::new(Some("archiveis (https://github.com/MattsSe/archiveis-rs)"));

    // the urls to capture
    let urls = vec![
        "http://example.com/",
        "https://github.com/MattsSe/archiveis-rs",
        "https://crates.io",
    ];

    let capture = client.capture_all(urls, None).and_then(|archives| {
        let failures: Vec<_> = archives
            .iter()
            .map(Result::as_ref)
            .filter(Result::is_err)
            .map(Result::unwrap_err)
            .collect();
        if failures.is_empty() {
            println!("all links successfully archived.");
        } else {
            for err in failures {
                if let archiveis::Error::MissingUrl(url) = err {
                    println!("Failed to archive url: {}", url);
                }
            }
        }
        Ok(())
    });
    core.run(capture).unwrap();
}
```

## Commandline Interface

`archiveis` also comes as commandline application:

Coming soon