use structopt::StructOpt;

use archiveis::{ArchiveClient, Archived};
use futures::future::Future;
use hyper::http::Uri;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
};

#[deny(warnings)]
#[allow(missing_docs)]
#[derive(Debug, StructOpt)]
#[structopt(
    name = "archive",
    about = "Archive urls using the archive.is capturing service."
)]
#[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
enum App {
    #[structopt(name = "links", about = "Archive all links provided as arguments")]
    Links {
        #[structopt(
            raw(required = "true"),
            short = "i",
            parse(try_from_str),
            help = "all links to should be archived via archive.is"
        )]
        links: Vec<Uri>,
        #[structopt(flatten)]
        opts: Opts,
    },
    #[structopt(
        name = "file",
        about = "Archive all the links in the line separated text file"
    )]
    File {
        #[structopt(
            short = "i",
            parse(from_os_str),
            help = "archive all the links in the line separated text file"
        )]
        input: PathBuf,
        #[structopt(flatten)]
        opts: Opts,
    },
}

#[derive(Debug, StructOpt)]
struct Opts {
    #[structopt(short = "o", parse(from_os_str), help = "save all archived elements")]
    output: Option<PathBuf>,
    #[structopt(long = "archives-only", help = "save only the archive urls")]
    archives_only: bool,
    #[structopt(
        short = "t",
        long = "text",
        help = "save output as line separated text instead of json"
    )]
    text: bool,
    #[structopt(
        short = "a",
        long = "append",
        help = "if the output file already exists, append instead of overwriting the file"
    )]
    append: bool,
    #[structopt(short = "s", long = "silent", help = "do not print anything")]
    silent: bool,
    #[structopt(
        short = "r",
        long = "retries",
        default_value = "0",
        help = "how many times failed archive attempts should be tried again"
    )]
    retries: usize,
    #[structopt(
        long = "ignore-failures",
        help = "continue anyway if after all retries some links are not successfully archived"
    )]
    ignore_failures: bool,
}

impl Opts {
    pub(crate) fn write_output(&self, archives: Vec<Output>) {
        use ::std::io::prelude::*;
        if let Some(out) = &self.output {
            let mut file = if self.append && out.exists() {
                fs::OpenOptions::new().write(true).append(true).open(out)
            } else {
                fs::File::create(out)
            }
            .expect(&format!("Failed to open file {}", out.display()));

            let len = archives.len();

            if self.text {
                for archive in archives {
                    let write = if self.archives_only {
                        writeln!(file, "{}", archive.archive)
                    } else {
                        writeln!(file, "{}\t{}", archive.target, archive.archive)
                    };

                    if let Err(e) = write {
                        if !self.silent {
                            eprintln!("Couldn't write to file: {}", e);
                        }
                    }
                }
            } else {
                let content = if self.archives_only {
                    let content: Vec<_> = archives.into_iter().map(|x| x.archive).collect();
                    serde_json::to_string_pretty(&content)
                } else {
                    serde_json::to_string_pretty(&archives)
                }
                .expect("Failed to convert to json.");
                if let Err(e) = write!(file, "{}", content) {
                    if !self.silent {
                        eprintln!("Couldn't write to file: {}", e);
                    }
                }
            }
            if !self.silent {
                println!("Wrote {} archived links to: {}", len, out.display());
            }
        }
    }
}

/// type for storing captures to a file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Output {
    /// The requested url to archive with the archive.is capture service
    target: String,
    /// The archive.is url that archives the `target_url`, if archive was successful
    archive: String,
}

impl From<Archived> for Output {
    fn from(archive: Archived) -> Self {
        Output {
            target: archive.target_url,
            archive: archive.archived_url,
        }
    }
}

fn main() -> Result<(), Box<dyn ::std::error::Error>> {
    pretty_env_logger::try_init()?;
    let app = App::from_args();

    let client = ArchiveClient::default();

    let (links, opts) = match app {
        App::File { input, opts } => {
            let reader = BufReader::new(
                fs::File::open(&input).expect(&format!("Cannot open {}", input.display())),
            );
            let links = reader
                .lines()
                .map(Result::unwrap)
                .map(|link| {
                    link.trim()
                        .parse::<Uri>()
                        .expect(&format!("Link {} is no valid uri.", link))
                })
                .collect::<Vec<_>>();
            (links, opts)
        }
        App::Links { links, opts } => (links, opts),
    };

    if links.is_empty() {
        if !opts.silent {
            eprintln!("Nothing to archive.");
        }
        ::std::process::exit(1);
    }

    let retries = opts.retries;

    let work = client
        .get_unique_token()
        .and_then(move |token| {
            capture_links(client, links.iter().map(|x| x.to_string()).collect(), token).and_then(
                move |(client, archives, token)| fold_captures(client, archives, retries, token),
            )
        })
        .map_err(|_| ())
        .and_then(move |archives| {
            if archives.iter().any(Result::is_err) && !opts.ignore_failures {
                if !opts.silent {
                    let failures: Vec<_> = archives
                        .into_iter()
                        .filter(Result::is_err)
                        .map(Result::unwrap_err)
                        .filter_map(|x| match x {
                            archiveis::Error::ServerError(url)
                            | archiveis::Error::MissingUrl(url) => Some(url),
                            _ => None,
                        })
                        .collect();
                    eprintln!("Failed to archive links: {:?}", failures);
                }
                Err(())
            } else {
                let successes: Vec<Output> = archives
                    .into_iter()
                    .filter_map(|x| {
                        if let Ok(archive) = x {
                            Some(archive.into())
                        } else {
                            None
                        }
                    })
                    .collect();

                if !opts.silent {
                    for success in &successes {
                        println!("Archived {}  -->  {}", success.target, success.archive);
                    }
                }

                opts.write_output(successes);

                Ok(())
            }
        });

    hyper::rt::run(work);

    Ok(())
}

/// returns a new future which will capture all links
fn capture_links(
    client: ArchiveClient,
    links: Vec<String>,
    token: String,
) -> impl Future<
    Item = (ArchiveClient, Vec<archiveis::Result<Archived>>, String),
    Error = archiveis::Error,
> + Send {
    let mut futures = Vec::with_capacity(links.len());
    for url in links {
        futures.push(
            client
                .capture_with_token(url.into(), token.clone())
                .then(Ok),
        );
    }
    futures::future::join_all(futures).and_then(|archives| Ok((client, archives, token)))
}

/// retries capturing until are `retries` are exhausted or every link was archived successfully.
fn fold_captures(
    client: ArchiveClient,
    archives: Vec<archiveis::Result<Archived>>,
    retries: usize,
    token: String,
) -> Box<dyn Future<Item = Vec<archiveis::Result<Archived>>, Error = archiveis::Error> + Send> {
    if archives.iter().all(Result::is_ok) || retries == 0 {
        Box::new(futures::future::ok(archives))
    } else {
        let (mut archived, failures): (Vec<_>, Vec<_>) =
            archives.into_iter().partition(Result::is_ok);

        let failures: Vec<_> = failures
            .into_iter()
            .map(Result::unwrap_err)
            .filter_map(|x| match x {
                archiveis::Error::ServerError(url) | archiveis::Error::MissingUrl(url) => Some(url),
                _ => None,
            })
            .collect();

        let work =
            capture_links(client, failures, token).and_then(move |(client, captures, token)| {
                archived.extend(captures.into_iter());
                fold_captures(client, archived, retries - 1, token)
            });

        Box::new(work)
    }
}
