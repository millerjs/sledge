//! Download files

use ::DEFAULT_BUFF_SIZE;
use ::errors::DownloadError;
use hyper::Client;
use hyper::client::response::Response;
use hyper::header::ContentDisposition;
use hyper::header::ContentLength;
use hyper::header::DispositionParam;
use hyper::header::Header;
use hyper::header::HeaderFormat;
use hyper::status::StatusCode;
use pbr::ProgressBar;
use pbr::Units;
use std::fs::File;
use std::io::Read;
use std::io;
use std::path::Path;
use std::str;


/// Download a list of urls
pub fn download_urls<H>(urls: Vec<String>, headers: Vec<H>)
                        -> Result<String, DownloadError>
    where H: Header + HeaderFormat
{
    let mut failed = vec![];
    let count = urls.len();

    for url in urls {
        match download_to_file(&*url, headers.clone()) {
            Ok(ok) => info!("{}\n", ok),
            Err(err) => {
                error!("{}\n", err);
                failed.push(url);
            },
        }
    }

    if failed.len() > 0 {
        Err(DownloadError(
            format!("Downloaded {} files successfully. Failed to download {}",
                    count - failed.len(), failed.join(", "))))
    } else {
        Ok(format!("All {} files downloaded successfully", count))
    }
}


/// Serially download file to file
pub fn download_to_file<H>(url: &str, headers: Vec<H>)
                           -> Result<String, DownloadError>
    where H: Header + HeaderFormat
{
    let request = try!(make_request(url, headers));

    match stream_response_to_file(request) {
        Ok(ok) => Ok(format!("Download complete: {}", ok)),
        Err(err) => Err(DownloadError(format!("Unable to download {}: {}", url, err))),
    }
}


/// Stream a successful response to a file
fn stream_response_to_file(mut response: Response)
                           -> Result<String, DownloadError>
{
    let file_size = try!(parse_file_size(&response));
    let mut file = try!(open_file_for_response(&response));

    match copy_with_progress(file_size, &mut response, &mut file) {
        Ok(bytes) => Ok(format!("Wrote {} bytes", bytes)),
        Err(err) => Err(DownloadError(format!("{}", err)))
    }
}


fn make_request<H>(url: &str, headers: Vec<H>)
                   -> Result<Response, DownloadError>
    where H: Header + HeaderFormat
{
    // Construct a request
    let client = Client::new();
    let mut request = client.get(url);

    // Contact API
    info!("Requesting {}", url);
    let mut response = try!(request.send());

    if response.status != StatusCode::Ok {
        let mut body = String::new();
        try!(response.read_to_string(&mut body));
        Err(DownloadError(format!("{:}: {}", response.status, body)))
    } else {
        debug!("Request to {} successful", url);
        Ok(response)
    }
}


/// Vendored io::copy() to report progress because <Write>.broadcast() was
/// deprecated in 1.6
pub fn copy_with_progress<R: ?Sized, W: ?Sized>(stream_size: u64, reader: &mut R, writer: &mut W)
                                                -> io::Result<u64>
    where R: io::Read, W: io::Write
{
    debug!("Stream is {} bytes", stream_size);
    let mut pb = ProgressBar::new(stream_size);
    pb.set_units(Units::Bytes);

    let mut buf = [0; DEFAULT_BUFF_SIZE];
    let mut written = 0;

    loop {
        let len = match reader.read(&mut buf) {
            Ok(0) => return Ok(written),
            Ok(len) => len,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        try!(writer.write_all(&buf[..len]));
        written += len as u64;
        pb.add(len as u64);
    }
}


/// Reads the filename from the Content-Disposition if possible
fn parse_file_name(response: &Response) -> Result<String, DownloadError>
{
    if let Some(disposition) = response.headers.get::<ContentDisposition>() {
        let file_name_param = disposition.parameters.iter()
            .map(|p| match *p {
                DispositionParam::Filename(_, _, ref bytes) => {
                    match str::from_utf8(bytes) {
                        Err(e) => { warn!("{}", e); None },
                        Ok(s) => Some(s),
                    }
                },
                _ => None
            }).filter_map(|p| p).nth(0);

        if let Some(file_name) = file_name_param {
            debug!("server provided filename: {}", file_name);
            return Ok(file_name.to_string())
        }
    }

    Err(DownloadError(format!("server url not provide a filename")))
}


/// Reads the file size from the Content-Length if possible
fn parse_file_size(response: &Response) -> Result<u64, DownloadError>
{
    match response.headers.get::<ContentLength>() {
        Some(size) => Ok(size.0),
        None => Err(DownloadError(
            "Server did not provide a content length!".to_owned())),
    }
}


/// Parse the file name (or use default name) and return an opened file
fn open_file_for_response(response: &Response)
                          -> Result<File, DownloadError>
{
    let file_name = match parse_file_name(&response) {
        Ok(name) => name,
        Err(e) => {
            let default = response.url.path_segments()
                .unwrap().collect::<Vec<_>>().last().unwrap().to_string();
            warn!("No filename ({}) downloading to {}", e, default);
            default
        }
    };

    let path = Path::new(&*file_name).file_name().unwrap();
    info!("Streaming data to {}", file_name);

    match File::create(path) {
        Ok(f) => Ok(f),
        Err(e) => Err(DownloadError(
            format!("Unable to open file {} for writing: {}", file_name, e))),
    }
}
