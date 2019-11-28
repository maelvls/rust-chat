#![recursion_limit = "1024"] // `error_chain!` can recurse deeply
#![feature(vec_remove_item)]

#[macro_use]
extern crate clap;
extern crate colored;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;

use colored::Colorize;
use log::{Level, LevelFilter, Metadata, Record};
use std::io::Read;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;

/// Allows us to use .chain_err(). See https://docs.rs/error-chain.
mod errors {
  error_chain! {}
}
use errors::*;

/// This Writer struct holds the information on a writer-thread so that
/// main_writer is able to send messages to all writers.
pub struct Writer {
  sender: mpsc::Sender<String>,
  id: usize,
}

/// Contains the action that main_writer should execute.
pub enum Action {
  ToWriters(String, Writer),
  AddWriter(Writer),
  RmWriter(Writer),
}
impl PartialEq for Writer {
  fn eq(self: &Self, rhs: &Self) -> bool {
    self.id == rhs.id
  }
}

/// Allows us to use `error!()`, `info!()`...
struct OurLogger;
impl log::Log for OurLogger {
  fn enabled(&self, _: &Metadata) -> bool {
    true
  }
  fn log(&self, rec: &Record) {
    if self.enabled(rec.metadata()) {
      match rec.level() {
        Level::Error => eprintln!("{} {}", "error:".red().bold(), rec.args()),
        Level::Warn => eprintln!("{} {}", "warn:".yellow().bold(), rec.args()),
        Level::Info => eprintln!("{} {}", "info:".yellow().bold(), rec.args()),
        Level::Debug => eprintln!("{} {}", "debug:".bright_black().bold(), rec.args()),
        Level::Trace => eprintln!("{} {}", "trace:".blue().bold(), rec.args()),
      }
    }
  }
  fn flush(&self) {}
}

fn main() {
  log::set_logger(&OurLogger).unwrap();
  log::set_max_level(LevelFilter::Trace);

  let args: clap::ArgMatches = clap_app!(rustchat =>
    (version: env!("CARGO_PKG_VERSION"))
    (about: env!("CARGO_PKG_DESCRIPTION"))
    (@subcommand client =>
        (about: "run as client")
        (@arg ADDRESS: +required "address to use")
        (@arg PORT: +required "Port"))
    (@subcommand server =>
        (about: "run as server")
        (@arg PORT: +required "Port"))
    (@setting TrailingVarArg) (@setting GlobalVersion)
    (@setting SubcommandRequiredElseHelp) (@setting DeriveDisplayOrder)
    // (@arg debug: -d ... "Sets the level of debugging information")
  )
  .get_matches();

  // This 'run' function is like 'main' except it allows us to return a
  // Result type so that we can handle gracefully errors using chain_err.
  let run = || -> Result<()> {
    match args.subcommand() {
      ("server", Some(subarg)) => {
        let port = subarg.value_of("PORT").unwrap();
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
          .chain_err(|| format!("port '{}' aleady used", port.yellow()))?;
        info!("listening started");

        let (reader_send, to_main_writer) = mpsc::channel();

        // The 'main_writer' is the one who takes input from one of
        // incoming connections (from one of the readers) and send them
        // along to the other connections (passing a message to all the
        // writers).
        let _main_writer = thread::spawn(move || -> Result<()> {
          let mut writers: Vec<Writer> = Vec::new();
          while let Some(act) = to_main_writer.recv().ok() {
            match act {
              Action::ToWriters(msg, from) => {
                writers.iter().filter(|to| **to != from).for_each(|to| {
                  debug!("ask writer n°{} to send '{}'", to.id, msg.yellow());
                  to.sender
                    .send(msg.clone()) // Idk why this clone is required
                    .unwrap_or_else(|_err| error!("cannot send to n°{}", to.id))
                })
              }
              Action::AddWriter(w) => writers.push(w),
              Action::RmWriter(w) => {
                writers.remove_item(&w);
              }
            }
          }
          Ok(())
        });

        for (id, stream) in listener.incoming().enumerate() {
          // Each reader thread must be able to send its received message
          // to the main_writer. As it is a all-to-one communication to the
          // main_chan, we can reuse the same sender.
          let reader_send = reader_send.clone();

          // On the contrary, sending a message from the main_writer to all
          // the writer threads is a one-to-all communication. As it is not
          // provided by the std lib, we will create one channel per writer.
          let (writer_send, writer_recv) = mpsc::channel();

          // Tell the main_writer that we got a new writer he should know of.
          reader_send
            .send(Action::AddWriter(Writer {
              sender: writer_send.clone(),
              id,
            }))
            .chain_err(|| "couldn't add writer to the main writer (wtf this err msg?)")?;

          info!("incoming connection n°{}", id);

          let mut writer: TcpStream = stream.unwrap();
          let reader: TcpStream = writer.try_clone().unwrap();

          // The writer for this incoming connection. He is responsible for
          // sending the messages given by main_writer to the connection.
          thread::spawn(move || -> Result<()> {
            writeln!(writer, "server: connected as n°{}", id).chain_err(|| "")?;
            loop {
              let msg = writer_recv.recv().chain_err(|| "writer errored")?;
              writeln!(writer, "{}", msg)
                .chain_err(|| format!("error writing to connection n°{}", id))?;
              debug!("writer n°{} emited '{}'", id, msg.yellow());
            }
          });

          // The reader for this incoming connection. He receives the messages
          // from the connection and passes them to the main_writer.
          thread::spawn(move || -> Result<()> {
            // Note: `Read::chars()` has been removed. See:
            // https://github.com/rust-lang/rust/issues/27802#issuecomment-377537778
            // I wanted a quick fix, so I used `::io::Read::read_to_string`
            // but it does allow replacing wrong utf-8 code points with the
            // replacement character (`�`). The good option would be to use
            // the utf8 crate and mimic the `reader.chars()` behaviour.
            //Replacement: reader.read_to_string(&mut buf);
            let reader_buf = BufReader::new(reader);
            for line in reader_buf.lines() {
              match line {
                Ok(l) => {
                  debug!("reader n°{} received '{}'", id, l.yellow());
                  let sender = writer_send.clone();
                  reader_send
                    .send(Action::ToWriters(l, Writer { sender, id }))
                    .chain_err(|| "")?;
                }
                Err(e) => {
                  error!(
                    "reader n°{} received a line with a wrong utf8 seq '{}'",
                    id, e
                  );
                }
              }
            }
            Ok(())
          });
        }
      }

      ("client", Some(subarg)) => {
        let (address, port) = (
          subarg.value_of("ADDRESS").unwrap(),
          subarg.value_of("PORT").unwrap(),
        );
        let reader = TcpStream::connect(format!("{}:{}", address, port)).chain_err(|| {
          format!(
            "could not connect to {}:{}",
            address.yellow(),
            port.yellow()
          )
        })?;
        let mut writer = reader
          .try_clone()
          .chain_err(|| "impossibe to clone the TCP stream (i.e., the socket")?;

        info!("you can start typing");
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
        thread::spawn(move || -> Result<()> {
          let reader_buf = BufReader::new(reader);
          for line in reader_buf.lines() {
            match line {
              Ok(l) => println!("{} {}", "remote:".blue().bold(), l),
              Err(e) => {
                error!(
                  "{} received a line with a wrong utf8 seq: '{}'",
                  "remote:".blue().bold(),
                  e
                );
              }
            }
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
    error!("{}", e);
    for e in e.iter().skip(1) {
      error!("{} {}", "caused by:".bright_black().bold(), e);
    }
    // Use `RUST_BACKTRACE=1` to enable the backtraces.
    if let Some(backtrace) = e.backtrace() {
      error!("{} {:?}", "backtrace:".blue().bold(), backtrace);
    }
    std::process::exit(1);
  }
}
