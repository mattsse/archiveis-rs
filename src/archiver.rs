use structopt::StructOpt;

use archiveis::ArchiveClient;
use futures::future::Future;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "archer",
    about = "Archive urls using the archive.is capturing service."
)]
#[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
enum App {
    /// See how the server is configured
    #[structopt(name = "config")]
    Config(Config),
    /// translate documents
    #[structopt(name = "translate")]
    Translate,
}

#[derive(Debug, StructOpt)]
enum Parse {
    #[structopt(name = "all")]
    All,
    #[structopt(name = "text")]
    Text,
    #[structopt(name = "meta")]
    Meta,
}

fn main() {
    // let app = App::from_args();

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
