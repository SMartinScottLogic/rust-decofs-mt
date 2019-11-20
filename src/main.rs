use std::{env, io};
use std::ffi::{OsStr, OsString};
use chrono::Local;
use std::path::PathBuf;

#[macro_use]
extern crate log;

mod deco;
mod libc_wrapper;

struct ConsoleLogger;

impl log::Log for ConsoleLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        println!("{} {} {} - {}",
            Local::now().format("%Y-%m-%dT%H:%M:%S%z"),
            record.level(),
            record.target(),
            record.args()
        );
     }

    fn flush(&self) {}
}

static LOGGER: ConsoleLogger = ConsoleLogger;

fn main() -> io::Result<()> {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Debug);
    
    let args: Vec<OsString> = env::args_os().collect();

    if args.len() != 3 {
        println!("usage: {} <target> <mountpoint>", &env::args().next().unwrap());
        ::std::process::exit(-1);
    }
    
    let filesystem = deco::DecoFS::new(PathBuf::from(args[1].clone()));
    let options = ["-o", "rw", "-o", "fsname=decofs", "-o", "allow_other", "-a", "auto_mount"]
        .iter()
        .map(|o| o.as_ref())
        .collect::<Vec<&OsStr>>();
    fuse_mt::mount(fuse_mt::FuseMT::new(filesystem, 1), &args[2], &options)
}
