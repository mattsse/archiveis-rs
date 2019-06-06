use structopt::StructOpt;

use archiveis::{ArchiveClient, Archived};
use futures::future::Future;
use hyper::http::Uri;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "archer",
    about = "Archive urls using the archive.is capturing service."
)]
#[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
#[allow(missing_docs)]
struct App {
    /// All links to archive
    #[structopt(
        parse(try_from_str),
        short = "l",
        help = "all links to should be archived via archive.is"
    )]
    links: Vec<Uri>,
    /// archive links from a file
    #[structopt(
        parse(from_os_str),
        short = "i",
        help = "archive all the links in the line separated file"
    )]
    input: Option<PathBuf>,
    #[structopt(short = "o", parse(from_os_str), help = "save all archived links")]
    output: Option<PathBuf>,
    #[structopt(short = "j", help = "save captured links as json")]
    json: bool,
    #[structopt(short = "s", help = "do not print anything")]
    silent: bool,
}

/// type for storing captures to a file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Output {
    /// The requested url to archive with the archive.is capture service
    target_url: String,
    /// The archive.is url that archives the `target_url`
    archived_url: String,
}

impl From<Archived> for Output {
    fn from(archive: Archived) -> Self {
        Output {
            target_url: archive.target_url,
            archived_url: archive.archived_url,
        }
    }
}

fn main()  {
    pretty_env_logger::try_init()?;
    let app = App::from_args();
    println!("{:?}", app);

    let client = ArchiveClient::default();

    if let Some(input) = &app.input {
        let reader =
            BufReader::new(File::open(input).expect(&format!("Cannot open {}", input.display())));
        let lines: Vec<_> = reader.lines().map(Result::unwrap).collect();


    }

    //    let client = ArchiveClient::default();
    //
        // the urls to capture
        let urls = vec![
            "http://example.com/",
            "https://github.com/MattsSe/archiveis-rs",
            "https://crates.io",
        ];

        let capture = client.capture_all(urls, None);

        hyper::rt::run(capture.map_err(|_| ()).and_then(|_| Ok(())));

}
