# archiveis-rs
[![Build Status](https://travis-ci.com/MattsSe/archiveis-rs.svg?branch=master)](https://travis-ci.com/MattsSe/archiveis-rs)

Provides simple access to the Archive.is Capturing Service.
Archive any url and get the corresponding archive.is link in return.

### Full example
The `ArchiveClient` is build with `hyper` and is build with `futures`.

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
     println!("url of archived site: {}", archived.unwrap().url);

     Ok(())
 });

 core.run(capture).unwrap();
}