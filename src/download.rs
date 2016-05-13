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
use std::cmp::min;
use std::fs::File;
use std::io::prelude::Seek;
use std::io::Read;
use std::io::Write;
use std::io;
use std::path::Path;
use std::str;
use std::thread;


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

    /// Set the mode of the Download
    pub fn mode(mut self, mode: DownloadMode) -> Download
    {
        self.mode = mode;
        self
    }

    /// Download the source to target base on the download mode
    pub fn download(&mut self) -> Result<u64, DownloadError>
    {
        match self.mode {
            DownloadMode::Serial => self.download_serial(),
            DownloadMode::Parallel(n) => self.download_parallel(n),
        }
    }

    /// Download the source to the target serially
    fn download_serial(&mut self) -> Result<u64, DownloadError>
    {
        info!("Downloading serially");
        let response  = try!(get(&*self.url, self.headers.clone()));
        let size = try!(parse_content_length(&response));
        try!(set_target_len(&self.target, size, &response));
        stream(&self.target, 0, response)
    }

    /// Download the source to the target in parallel
    fn download_parallel(&mut self, n: u8) -> Result<u64, DownloadError>
    {
        info!("Downloading with {} threads", n);

        let head = try!(head(&*self.url, self.headers.clone()));
        let size = try!(parse_content_length(&head));
        let block_size = size / (n as u64);

        try!(set_target_len(&self.target, size, &head));

        let mut children = vec![];
        for i in 0..n {
            let headers = self.headers.clone();
            let target = self.target.clone();
            let url = self.url.clone();

            let start = min(i as u64 * block_size, size);
            let end = min((i as u64 + 1) * block_size, size);

            children.push(thread::spawn(move || {
                debug!("Making request for segment ({} - {})", start, end);
                let response = try!(get(&*url, headers));
                stream(&target, start, response)
            }))
        };

        debug!("Joining.");

        for child in children {
            let _ = child.join();
        }

        Ok(size)
    }
}

/// Construct and execute GET request against API
fn get(url: &str, headers: Headers) -> Result<Response, DownloadError>
{
    let client = Client::new();
    let request = client.get(&*url).headers(headers);
    raise_for_status(try!(request.send()))
}

/// Construct and execute HEAD request against API
fn head(url: &str, headers: Headers) -> Result<Response, DownloadError>
{
    let client = Client::new();
    let request = client.head(url).headers(headers);
    raise_for_status(try!(request.send()))
}

/// Returns error if request unsuccessful
fn raise_for_status(mut response: Response) -> Result<Response, DownloadError>
{
    if response.status != StatusCode::Ok {
        let mut body = String::new();
        try!(response.read_to_string(&mut body));
        Err(DownloadError(format!("{:}: {}", response.status, body)))
    } else {
        debug!("Request to {} successful", response.url);
        Ok(response)
    }
}

/// Set the expected length of the target (if applicable)
fn set_target_len(target: &DownloadTarget, size: u64, response: &Response)
                  -> Result<(), DownloadError>
{
    info!("Setting the length of target to {} bytes", size);
    match *target {
        DownloadTarget::Default => {
            let file = try!(open_default_file_target(response));
            Ok(try!(file.set_len(size)))
        },
        DownloadTarget::File(ref path) => {
            let file = try!(File::open(path));
            Ok(try!(file.set_len(size)))
        },
        DownloadTarget::StdOut => {
            Err(DownloadError("Cannot take offset on stdout".to_owned()))
        }
    }
}

/// Stream the response to the download target at a given offset (if applicable)
fn stream(target: &DownloadTarget, offset: u64, mut response: Response)
          -> Result<u64, DownloadError>
{
    let size = try!(parse_content_length(&response));
    Ok(match *target {
        DownloadTarget::Default => {
            let mut file = try!(open_default_file_target(&response));
            try!(file.seek(io::SeekFrom::Start(offset)));
            try!(io::copy(&mut response, &mut file))
        },
        DownloadTarget::File(ref path) => {
            let mut file = try!(File::open(path));
            try!(file.seek(io::SeekFrom::Start(offset)));
            try!(io::copy(&mut response, &mut file))
        },
        DownloadTarget::StdOut => {
            try!(copy(size, &mut response, &mut io::stdout()))
        }
    })
}


/// Vendored io::copy() to report progress because <Write>.broadcast() was
/// deprecated in 1.6
pub fn copy<R: ?Sized, W: ?Sized>(
    size: u64,
    reader: &mut R,
    writer: &mut W,
) -> io::Result<u64>
    where R: io::Read, W: io::Write
{
    debug!("Stream is {} bytes", size);

    let mut buf = [0; DEFAULT_BUFF_SIZE];
    let mut pb = ProgressBar::new(size);
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
fn parse_content_length(response: &Response) -> Result<u64, DownloadError>
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
            debug!("no filename ({}) downloading to {}", e, default);
            default
        }
    };

    let path = Path::new(&*file_name).file_name().unwrap();
    debug!("opening {}", file_name);

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
