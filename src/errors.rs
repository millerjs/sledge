use std::io;
use std::fmt;
use hyper;

#[derive(Debug)]
pub struct DownloadError(pub String);

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<io::Error> for DownloadError {
    fn from(err: io::Error) -> DownloadError {
        DownloadError(err.to_string())
    }
}

impl From<hyper::Error> for DownloadError {
    fn from(err: hyper::Error) -> DownloadError {
        DownloadError(err.to_string())
    }
}
