#![feature(io)]
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
use std::char::REPLACEMENT_CHARACTER;

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
      (@subcommand client =>
          (about: "run as client")
          (@arg IPV4: +required "IP address to use")
          (@arg PORT: +required "Port"))
      (@subcommand server =>
          (about: "run as server")
          (@arg PORT: +required "Port"))
      (@setting TrailingVarArg) (@setting GlobalVersion)
      (@setting SubcommandRequiredElseHelp) (@setting DeriveDisplayOrder)
      // (@arg debug: -d ... "Sets the level of debugging information")
    ).get_matches();

  // This 'run' function is like 'main' except it allows us to return a
  // Result type so that we can handle gracefully errors using chain_err.
  let run = || -> Result<()> {
    match args.subcommand() {
      ("server", Some(subarg)) => {
        let port = subarg.value_of("PORT").unwrap();
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
          .chain_err(|| format!("port '{}' aleady used", port.yellow()))?;
        println!("{} listening started", "ready:".green().bold());
        for stream in listener.incoming() {
          println!("{} incoming connection", "note:".yellow().bold());
          let mut writer = stream.unwrap();
          let reader : TcpStream = writer.try_clone().unwrap();
          // The writer.
          // We use 'move' because 'writer' will be mutated. Each thread
          // returns an Option<Error> so that we can know when they errored.
          thread::spawn(move || -> Result<()> {
            writer
              .write(format!("{} ready!\r\n", "server:".purple()).as_bytes())
              .chain_err(|| "could not write to client")?;
            let stdin = std::io::stdin();
            for b in stdin.bytes() {
              let b = b.chain_err(|| "failed to read byte from stdin")?;
              writer
                .write(&[b])
                .chain_err(|| format!("failed to write byte '{}'", b as char))?;
            }
            Ok(())
          });
          // The reader.
          thread::spawn(|| -> Result<()> {
            for c in reader.chars() {
              print!("{}", c.unwrap_or(REPLACEMENT_CHARACTER))
            }
            // decode_utf8(reader.bytes().map(|f| f).into_iter())
            //   .for_each(|c| ;
            Ok(())
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
        // The writer.
        let thread_writer = thread::spawn(move || -> Result<()> {
          let stdin = std::io::stdin();
          for b in stdin.bytes() {
            let b = b.chain_err(|| "failed to read byte from stdin")?;
            writer
              .write(&[b])
              .chain_err(|| format!("failed to write byte '{}'", b as char))?;
          }
          Ok(())
        });
        // The reader.
        thread::spawn(|| -> Result<()> {
          for c in reader.chars() {
              print!("{}", c.unwrap_or(REPLACEMENT_CHARACTER))
          }
          Ok(())
        });
        // We must wait for the writing thread to terminate; otherwise,
        // the program will quit immediately.
        thread_writer
          .join()
          .unwrap()
          .chain_err(|| "writing thread errored")?;
      }
      _ => panic!("tell the dev: 'clap' should have ensured a subcommand is given"),
    }
    Ok(())
  };

  // Run and handle errors.
  if let Err(ref e) = run() {
    use std::io::Write;
    let stderr = &mut ::std::io::stderr();
    let errmsg = "Error writing to stderr";
    writeln!(stderr, "{} {}", "error:".red().bold(), e).expect(errmsg);
    for e in e.iter().skip(1) {
      writeln!(stderr, "{} {}", "caused by:".bright_black().bold(), e).expect(errmsg);
    }
    // Use `RUST_BACKTRACE=1` to enable the backtraces.
    if let Some(backtrace) = e.backtrace() {
      writeln!(stderr, "{} {:?}", "backtrace:".blue().bold(), backtrace).expect(errmsg);
    }
    ::std::process::exit(1);
  }
}
