#[macro_use]
extern crate log;
#[macro_use]
extern crate hyper;
extern crate clap;
extern crate env_logger;
extern crate sledge;

use std::env;
use hyper::header::Headers;

use clap::{
    App,
    Arg,
    ArgMatches,
};

use sledge::download::{
    Download,
    DownloadMode,
    DownloadTarget,
};

use sledge::reporter::ProgressBarReporter;


/// Setup logging (cli arg overwrites env var for dtt crate)
pub fn setup_logging(matches: &ArgMatches)
{
    let rust_log = env::var("RUST_LOG").unwrap_or("".to_owned());
    let log_level = match matches.occurrences_of("v") {
        0 => "sledge=info",
        _ => "sledge=debug",
    };

    env::set_var("RUST_LOG", &*format!("{},{}", rust_log, log_level));
    env_logger::init().unwrap();
    debug!("Set log level to {}", log_level);
}

fn main() {
    let matches = App::new("sledge")
        .version("0.1.0")
        .author("Joshua Miller <jsmiller@uchicago.edu>")
        .about("Parallel, resumable downloads.")
        .arg(Arg::with_name("URL")
             .help("URL to download")
             .required(true))
        .arg(Arg::with_name("THREADS")
             .short("n")
             .long("threads")
             .takes_value(true)
             .help("Number of threads to use during download"))
        .arg(Arg::with_name("v")
             .short("v")
             .multiple(true)
             .help("Sets the level of verbosity"))
        .get_matches();

    setup_logging(&matches);

    let url = matches.value_of("URL").unwrap().to_owned();

    let mode = match matches.value_of("THREADS").unwrap_or("1").parse::<u8>() {
        Ok(n) if n == 1 => DownloadMode::Serial,
        Ok(n) => DownloadMode::Parallel(n),
        Err(e) => return error!("Value for -n/--threads must be an integer: {}", e),
    };

    let result = Download::<ProgressBarReporter>::new(url.clone())
        .headers(Headers::new())
        .mode(mode)
        .target(DownloadTarget::Default)
        .download();

    match result {
        Err(err) => error!("Unable to download {}: {}\n", url, err),
        Ok(bytes) => info!("Download complete. Wrote {} bytes.\n", bytes),
    }
}
