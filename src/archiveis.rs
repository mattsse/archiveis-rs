use structopt::StructOpt;

use archiveis::{ArchiveClient, Archived};
use futures::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
};
use url::Url;

#[deny(warnings)]
#[allow(missing_docs)]
#[derive(Debug, StructOpt)]
#[structopt(
    name = "archive",
    about = "Archive urls using the archive.is capturing service."
)]
#[structopt(setting = structopt::clap::AppSettings::ColoredHelp)]
enum App {
    #[structopt(name = "links", about = "Archive all links provided as arguments")]
    Links {
        #[structopt(
            short = "i",
            parse(try_from_str),
            help = "all links to should be archived via archive.is"
        )]
        links: Vec<Url>,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
                        .parse::<Url>()
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

    let token = client.get_unique_token().await?;
    let archives = stream::iter(
        links
            .into_iter()
            .map(|url| async { client.capture_with_token(url, &token).await }),
    )
    .buffer_unordered(10)
    .collect::<Vec<_>>()
    .await;

    let archives = retry(&client, archives, opts.retries).await;

    if archives.iter().any(Result::is_err) && !opts.ignore_failures {
        if !opts.silent {
            let failures: Vec<_> = archives
                .into_iter()
                .filter(Result::is_err)
                .map(Result::unwrap_err)
                .filter_map(|x| match x {
                    archiveis::Error::ServerError(url) | archiveis::Error::MissingUrl(url) => {
                        Some(url)
                    }
                    _ => None,
                })
                .collect();
            eprintln!("Failed to archive links: {:?}", failures);
        }
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
    }

    Ok(())
}

/// retries capturing until are `retries` are exhausted or every link was archived successfully.
async fn retry(
    client: &ArchiveClient,
    archives: Vec<archiveis::Result<Archived>>,
    mut retries: usize,
) -> Vec<archiveis::Result<Archived>> {
    let (mut archived, mut failures): (Vec<_>, Vec<_>) =
        archives.into_iter().partition(Result::is_ok);
    while retries > 0 || !failures.is_empty() {
        for idx in (0..failures.len()).rev() {
            let failure = failures.swap_remove(idx).unwrap_err();
            let url = match failure {
                archiveis::Error::ServerError(url) | archiveis::Error::MissingUrl(url) => Some(url),
                _ => continue,
            };

            if let Some(url) = url {
                if let Ok(archive) = client.capture(&url).await {
                    archived.push(Ok(archive))
                } else {
                    failures.push(Err(archiveis::Error::MissingUrl(url)))
                }
            }
            retries -= 1;
        }
    }
    archived.extend(failures.into_iter());
    archived
}
