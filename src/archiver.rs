use archiveis::ArchiveClient;
use futures::future::Future;

fn main() {
    let client = ArchiveClient::default();

    // the urls to capture
    let urls = vec![
        "http://example.com/",
        "https://github.com/MattsSe/archiveis-rs",
        "https://crates.io",
    ];

    let cap = client.capp("", "");

    hyper::rt::run(capture.map_err(|_| ()).and_then(|_| Ok(())));
}
