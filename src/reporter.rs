use std::sync::mpsc::Receiver;
use pbr::{ProgressBar, Units};


#[derive(Debug)]
pub struct CompletedSegment {
    pub start: u64,
    pub len: u64,
    pub md5: String,
}

pub trait Reporter {
    fn new() -> Self;
    fn listen(&self, size: u64, receiver: Receiver<CompletedSegment>);
}

pub struct ProgressBarReporter;

impl Reporter for ProgressBarReporter {

    fn new() -> ProgressBarReporter
    {
        ProgressBarReporter
    }

    fn listen(&self, size: u64, receiver: Receiver<CompletedSegment>)
    {
        let mut pb = ProgressBar::new(size);
        pb.set_units(Units::Bytes);
        for segment in receiver {
            pb.add(segment.len);
        }
    }
}
