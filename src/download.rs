//! Download files


use ::DEFAULT_BUFF_SIZE;
use ::errors::DownloadError;
use hyper::Client;
use hyper::client::response::Response;
use hyper::header::ContentDisposition;
use hyper::header::ContentLength;
use hyper::header::DispositionParam;
use hyper::header::Headers;
use hyper::status::StatusCode;
use pbr::ProgressBar;
use pbr::Units;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::io;
use std::path::Path;
use std::str;


#[derive(Clone)]
pub enum DownloadTarget {
    /// Download the file to a given path
    File(String),
    /// Download the file to stdout
    StdOut,
    /// Download the file to a path specified by the server or based
    /// on the url
    Default,
}

#[derive(Clone)]
pub enum DownloadMode {
    /// Download the file serially
    Serial,
    /// Download the file in parallel (not implemented)
    Parallel(u8),
}

#[allow(dead_code)]
pub struct Download {
    /// The url to download from
    url: String,
    /// The path to stream the download to
    target: DownloadTarget,
    /// Headers to be applied to the request
    headers: Headers,
    /// The mode in which this file will de downloaded
    mode: DownloadMode,
}


impl Download {

    /// Create a new Download
    pub fn new(url: String) -> Download {
        Download {
            headers: Headers::new(),
            mode: DownloadMode::Serial,
            url: url,
            target: DownloadTarget::Default,
        }
    }

    /// Set the headers of the Download
    pub fn headers(mut self, headers: Headers) -> Download
    {
        self.headers = headers;
        self
    }

    /// Set the target of the Download
    pub fn target(mut self, target: DownloadTarget) -> Download
    {
        self.target = target;
        self
    }

    /// Construct and execute request against API to stream data to
    /// the Download target
    fn make_request(&mut self) -> Result<Response, DownloadError>
    {
        let client = Client::new();
        let request = client.get(&*self.url).headers(self.headers.clone());

        // Contact API
        info!("Requesting {}", self.url);
        let mut response = try!(request.send());

        // Handle response
        if response.status != StatusCode::Ok {
            let mut body = String::new();
            try!(response.read_to_string(&mut body));
            Err(DownloadError(format!("{:}: {}", response.status, body)))
        } else {
            debug!("Request to {} successful", self.url);
            Ok(response)
        }
    }

    /// Stream a successful response to a file
    fn stream(&mut self) -> Result<u64, DownloadError>
    {
        let mut response = try!(self.make_request());
        let file_size = try!(parse_file_size(&response));

        Ok(match self.target {
            DownloadTarget::Default => {
                let mut file = try!(open_default_file_target(&response));
                try!(copy_with_progress(file_size, &mut response, &mut file))
            },
            DownloadTarget::File(ref path) => {
                let mut file = try!(File::open(path));
                try!(copy_with_progress(file_size, &mut response, &mut file))
            },
            DownloadTarget::StdOut => {
                try!(copy_with_progress(file_size, &mut response, &mut io::stdout()))
            }
        })
    }
}


/// Download a list of urls
pub fn download_urls(urls: Vec<String>, target: DownloadTarget, headers: Headers)
                     -> Result<String, DownloadError>
{
    let mut failed: Vec<String> = vec![];
    let count = urls.len();

    for url in urls {

        let mut download = Download::new(url.clone())
            .headers(headers.clone())
            .target(target.clone());

        match download.stream() {
            Err(err) => {
                error!("Unable to download {}: {}\n", url, err);
                failed.push(url);
            },
            Ok(bytes) => info!("Download complete. Wrote {} bytes.\n", bytes),
        }
    }

    if failed.len() > 0 {
        Err(DownloadError(format!(
            "Downloaded {} files successfully. Failed to download {}",
            count - failed.len(), failed.join(", "))))

    } else {
        Ok(format!("All {} files downloaded successfully", count))
    }
}


/// Vendored io::copy() to report progress because <Write>.broadcast() was
/// deprecated in 1.6
pub fn copy_with_progress<R: ?Sized, W: ?Sized>(stream_size: u64, reader: &mut R, writer: &mut W)
                                                -> io::Result<u64>
    where R: io::Read, W: io::Write
{
    debug!("Stream is {} bytes", stream_size);

    let mut buf = [0; DEFAULT_BUFF_SIZE];
    let mut pb = ProgressBar::new(stream_size);
    let mut written = 0;

    pb.set_units(Units::Bytes);

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


/// Reads the file size from the Content-Length if possible
fn parse_file_size(response: &Response) -> Result<u64, DownloadError>
{
    match response.headers.get::<ContentLength>() {
        Some(size) => Ok(size.0),
        None => Err(DownloadError(format!("server did not provide a content length!"))),
    }
}


/// Parse the file name (or use default name) and return an opened file
fn open_default_file_target(response: &Response) -> Result<File, DownloadError>
{
    let file_name = match parse_file_name(&response) {
        Ok(name) => name,
        Err(e) => {
            let default = response.url.path_segments()
                .unwrap().collect::<Vec<_>>().last().unwrap().to_string();
            warn!("no filename ({}) downloading to {}", e, default);
            default
        }
    };

    let path = Path::new(&*file_name).file_name().unwrap();
    info!("opening {} to stream data", file_name);

    match File::create(path) {
        Ok(f) => Ok(f),
        Err(e) => Err(DownloadError(
            format!("unable to open file {} for writing: {}", file_name, e))),
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

    Err(DownloadError(format!("server did not provide a file name")))
}
