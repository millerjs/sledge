#[macro_use]
extern crate log;
#[macro_use]
extern crate hyper;
extern crate clap;
extern crate env_logger;
extern crate dtt;

use clap::App;
use clap::Arg;
use clap::ArgMatches;
use clap::SubCommand;
use std::env;
use std::process::exit;


header! { (XAuthToken, "X-Auth-Token") => [String] }


fn main() {
    let matches = App::new("gdc-data-transfer-tool")
        .version("0.1.0")
        .author("Joshua Miller <jsmiller@uchicago.edu>")
        .about("GDC Data Transfer Tool")
        .subcommand(SubCommand::with_name("download")
                    .about("Download files from the GDC")
                    .arg(Arg::with_name("UUIDS")
                         .help("File UUIDs to download")
                         .multiple(true))
                    .arg(Arg::with_name("MANIFEST")
                         .short("m")
                         .long("manifest")
                         .takes_value(true)
                         .help("Path to manifest with file UUIDs to download"))
                    .arg(Arg::with_name("TOKEN")
                         .short("t")
                         .long("token")
                         .help("Auth token")
                         .takes_value(true))
                    .arg(Arg::with_name("HOST")
                         .short("H")
                         .long("host")
                         .help("Host of the API to download from")
                         .takes_value(true))
                    .arg(Arg::with_name("v")
                         .short("v")
                         .multiple(true)
                         .help("Sets the level of verbosity")))
        .subcommand(SubCommand::with_name("upload")
                    .about("Uploads files to the GDC"))
        .get_matches();

    setup_logging(&matches);

    // Download
    if let Some(matches) = matches.subcommand_matches("download") {
        download(matches);

    } else if let Some(_) = matches.subcommand_matches("upload") {
        error!("Upload functionality is not yet implemented");
        exit(1)

    } else {
        error!("Please specify a subcommand. For more information try --help");
        exit(1);
    }
}

/// Extract args to start a download session
fn download(matches: &ArgMatches)
{
    let host = &*matches.value_of("HOST").unwrap_or(dtt::DEFAULT_HOST);
    let token = matches.value_of("TOKEN");

    let mut dids = match matches.values_of("UUIDS") {
        Some(ids) => ids.map(|s| s.to_string()).collect(),
        None => vec![],
    };

    if let Some(path) = matches.value_of("MANIFEST") {
        match dtt::download::load_ids_from_manifest(path) {
            Ok(manifest_ids) => dids.extend(manifest_ids),
            Err(e) => { error!("Unable to read manifest '{}': {}", path, e); exit(1); }
        }
    }

    if dids.len() == 0 {
        return error!("No ids to download.")
    }

    let urls = dids.iter().map(|did| format!("{}/data/{}", host, did)).collect();
    let headers = match token {
        Some(t) => vec![XAuthToken(t.to_owned())],
        None => vec![],
    };

    match dtt::download::download_urls(urls, headers) {
        Ok(ok) => info!("{}\n", ok),
        Err(err) => error!("{}\n", err),
    }
}


/// Setup logging (cli arg overwrites env var for dtt crate)
fn setup_logging(matches: &ArgMatches)
{
    let rust_log = env::var("RUST_LOG").unwrap_or("".to_owned());
    let log_level = match matches.occurrences_of("v") {
        0 => "info",
        _ => "debug"
    };

    env::set_var("RUST_LOG", &*format!("{},dtt={}", rust_log, log_level));
    env_logger::init().unwrap();
}
