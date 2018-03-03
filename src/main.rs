#![recursion_limit = "1024"] // `error_chain!` can recurse deeply
#[macro_use]
extern crate clap;
extern crate colored;
#[macro_use]
extern crate error_chain;

use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::thread;

use colored::*;

// We'll put our errors in an `errors` module, and other modules in
// this crate will `use errors::*;` to get access to everything
// `error_chain!` creates.
mod errors {
  // Create the Error, ErrorKind, ResultExt, and Result types
  error_chain!{}
}
// This only gives access within this module. Make this `pub use errors::*;`
// instead if the types must be accessible from other modules (e.g., within
// a `links` section).
use errors::*;

fn main() {
  let args: clap::ArgMatches = clap_app!(rustchat =>
      (version: env!("CARGO_PKG_VERSION"))
      (about: env!("CARGO_PKG_DESCRIPTION"))
      (@setting TrailingVarArg)
      (@setting SubcommandRequiredElseHelp)
      (@setting ColorAuto)
      (@setting GlobalVersion)
      (@setting DeriveDisplayOrder)
      (@setting UnifiedHelpMessage)
      (@subcommand client =>
          (about: "run as client")
          (@arg IPV4: +required "IP address to use")
          (@arg PORT: +required "Port"))
      (@subcommand server =>
          (about: "run as server")
          (@arg PORT: +required "Port"))
      // (@arg debug: -d ... "Sets the level of debugging information")
    ).get_matches();

  // This 'run' function is like 'main' except it allows us to return a
  // Result type so that we can handle gracefully errors using chain_err.
  let run = || -> Result<()> {
    match args.subcommand() {
      ("server", Some(subarg)) => {
        let port = subarg.value_of("PORT").unwrap();
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).chain_err(|| {
          format!(
            "Port {} aleady used. Maybe server already running?",
            port.yellow()
          )
        })?;
        println!("{} listening started", "ready:".green().bold());
        for stream in listener.incoming() {
          println!("{} incoming connection", "note:".yellow().bold());
          let mut writer = stream.unwrap();
          let reader = writer.try_clone().unwrap();
          // writer
          thread::spawn(move || {
            // 'move' because s_write will be mutated
            writer
              .write(format!("{} ready!\r\n", "server:".purple()).as_bytes())
              .unwrap();
          });
          // reader
          thread::spawn(|| {
            for b in reader.bytes() {
              match b {
                Err(e) => (println!("{} {:?}", "read error:".red().bold(), e)),
                Ok(b) => print!("{}", b as char),
              }
            }
          });
        }
      }
      ("client", Some(subarg)) => {
        let (ipv4, port) = (
          subarg.value_of("IPV4").unwrap(),
          subarg.value_of("PORT").unwrap(),
        );
        let reader = TcpStream::connect(format!("{}:{}", ipv4, port))
          .chain_err(|| format!("could not connect to {}:{}", ipv4.yellow(), port.yellow()))?;
        let mut writer = reader
          .try_clone()
          .chain_err(|| "impossibe to clone the TCP stream (i.e., the socket")?;

        println!("{} you can start typing", "ready:".green().bold());
        // writer
        let thd_read = thread::spawn(move || {
          let stdin = std::io::stdin();
          stdin.bytes().for_each(|b| {
            let b = b.unwrap();
            writer.write(&[b]).unwrap();
          });
        });
        // reader
        let thd_write = thread::spawn(|| {
          for b in reader.bytes() {
            match b.unwrap() as char {
              b => print!("{}", b),
            }
          }
        });
        thd_write.join().unwrap();
        thd_read.join().unwrap();
        println!("{} both reader and writer started", "ready:".green().bold());
      }
      _ => panic!("tell the dev: 'clap' should have ensured a subcommand is given"),
    }
    Ok(())
  };

  // run and handle errors
  if let Err(ref e) = run() {
    use std::io::Write;
    let stderr = &mut ::std::io::stderr();
    let errmsg = "Error writing to stderr";

    writeln!(stderr, "{} {}", "error:".red().bold(), e).expect(errmsg);

    for e in e.iter().skip(1) {
      writeln!(stderr, "{} {}", "caused by:".bright_black().bold(), e).expect(errmsg);
    }
    if let Some(backtrace) = e.backtrace() {
      // The backtrace is not always generated. Try to run this example
      // with `RUST_BACKTRACE=1`.
      writeln!(stderr, "{} {:?}", "backtrace:".blue().bold(), backtrace).expect(errmsg);
    }
    ::std::process::exit(1);
  }
}
