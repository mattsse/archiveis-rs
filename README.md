# archiveis-rs

[![Build Status](https://travis-ci.com/MattsSe/archiveis-rs.svg?branch=master)](https://travis-ci.com/MattsSe/archiveis-rs)
[![Crates.io](https://img.shields.io/crates/v/archiveis.svg)](https://crates.io/crates/archiveis)
[![Documentation](https://docs.rs/archiveis/badge.svg)](https://docs.rs/archiveis)

Provides simple access to the Archive.is Capturing Service.
Archive any url and get the corresponding archive.is link in return.

## Examples

The `ArchiveClient` is build with `hyper` and uses futures for capturing archive.is links.

```rust
use archiveis::ArchiveClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = ArchiveClient::default();
    let archived = client.capture("http://example.com/").await?;
    println!("targeted url: {}", archived.target_url);
    println!("url of archived site: {}", archived.archived_url);
    println!("archive.is submit token: {}", archived.submit_token);
    Ok(())
}
```

### Archive multiple urls
archive.is uses a temporary token to validate a archive request.
The `ArchiveClient` `capture` function first obtains a new submit token via a GET request. The token is usually valid several minutes, and even if archive.is switched to a new in the meantime token,the older ones are still valid. So if we need to archive multiple links, we can only need to obtain the token once and then invoke the capturing service directly with `capture_with_token` for each url. `capture_all` returns a Vec of Results of every capturing request, so every single capture request gets executed regardless of the success of prior requests.


```rust 
use archiveis::ArchiveClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = ArchiveClient::default();
    
    // the urls to capture
    let urls = vec![
        "http://example.com/",
        "https://github.com/MattsSe/archiveis-rs",
        "https://crates.io",
    ];
    
    let (archived, failures) : (Vec<_>, Vec<_>) = client.capture_all(urls).await?.into_iter()
                .partition(Result::is_ok);
    
    let archived: Vec<_> = archived.into_iter().map(Result::unwrap).collect();
    let failures: Vec<_> = failures.into_iter().map(Result::unwrap_err).collect();
    if failures.is_empty() {
        println!("all links successfully archived.");
    } else {
        for err in &failures {
            if let archiveis::Error::MissingUrl(url) | archiveis::Error::ServerError(url) = err {
                println!("Failed to archive url: {}", url);
            }
        }
    }
    Ok(())
}
```

## Commandline Application

Archive links using the `archiveis` commandline application

### Install

```shell
cargo install archiveis --features cli
```

### Usage
```shell
SUBCOMMANDS:
    file     Archive all the links in the line separated text file
    links    Archive all links provided as arguments
```

The `file` and `links` subcommands take the same flags and options (besides there primary target = links or a file)

```shell
USAGE:
    archiveis links [FLAGS] [OPTIONS] -i <links>...

FLAGS:
    -a, --append             if the output file already exists, append instead of overwriting the file
        --archives-only      save only the archive urls
    -h, --help               Prints help information
        --ignore-failures    continue anyway if after all retries some links are not successfully archived
    -s, --silent             do not print anything
    -t, --text               save output as line separated text instead of json
    -V, --version            Prints version information

OPTIONS:
    -i <links>...          all links to should be archived via archive.is
    -o <output>            save all archived elements
    -r, --retries <retries>    how many times failed archive attempts should be tried again [default: 0]
```

Archive a set of links:

```shell
archiveis links -i "http://example.com/" "https://github.com/MattsSe/archiveis-rs"
```

Archive a set of links and safe result to `archived.json`, retry failed attempts twice:

```shell
archiveis links -i "http://example.com/" "https://github.com/MattsSe/archiveis-rs" -o archived.json --retries 2
```

Archive all line separated links in file `links.txt` and only safe the archive urls line separated to `archived.txt`

```shell
archiveis file -i links.txt -o archived.txt --text --archives-only
```

By default `archiveis` aborts and doesn't output anything if there are still failed archive attempts after all retries. To ignore failures add the `--ignore-failures` flag to write output without the failures.

```shell
archiveis file -i links.txt -o archived.json --ignore-failures
```


## License

Licensed under either of these:

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   https://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   https://opensource.org/licenses/MIT)
