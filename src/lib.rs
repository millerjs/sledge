#![crate_name = "sledge"]

#[macro_use]
extern crate hyper;
#[macro_use]
extern crate log;
extern crate pbr;

extern crate env_logger;

pub const DEFAULT_BUFF_SIZE: usize = 1 * 1024 * 1024;  // 1 MB

pub mod download;
pub mod errors;
pub mod reporter;
