#![feature(io)]
#![recursion_limit = "1024"] // `error_chain!` can recurse deeply
#![feature(vec_remove_item)]

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
use std::sync::mpsc;

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

/// A Sender. Allows us to remember to which channel we should write.
pub struct Writer {
  sender: mpsc::Sender<String>,
  id: usize,
}

/// An action to send to the main_writer.
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

        let (reader_send, mut to_main_writer) = mpsc::channel();

        // The 'main_writer' is the one who takes input from one of
        // incoming connections (from one of the readers) and send them
        // along to the other connections (passing a message to all the
        // writers).
        let _main_writer = thread::spawn(move || -> Result<()> {
          let mut writers: Vec<Writer> = Vec::new();
          while let Some(act) = to_main_writer.recv().ok() {
            match act {
              Action::ToWriters(msg, from) => {
                writers.iter().filter(|writer| **writer != from).for_each(|writer| {
                  println!(
                    "{} ask writer n°{} to send '{}'",
                    "debug:".bright_black().bold(),
                    writer.id,
                    msg.yellow()
                  );
                  writer
                    .sender
                    .send(msg.clone()) // TODO: why is clone required?
                    .unwrap_or_else(|_err| eprintln!("{} cannot send to n°{}", "error:".red().bold(), writer.id))
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

          println!("{} incoming connection n°{}", "note:".yellow().bold(), id);

          let mut writer: TcpStream = stream.unwrap();
          let mut reader: TcpStream = writer.try_clone().unwrap();

          // The writer for this incoming connection. He is responsible for
          // sending the messages given by main_writer to the connection.
          thread::spawn(move || -> Result<()> {
            writeln!(writer, "server: connected as n°{}", id).chain_err(|| "")?;
            loop {
              let msg = writer_recv.recv().chain_err(|| "writer errored")?;
              writeln!(writer, "{}", msg)
                .chain_err(|| format!("error writing to connection n°{}", id))?;
              println!(
                "{} writer n°{} emited '{}'",
                "debug:".bright_black().bold(),
                id,
                msg.yellow()
              );
            }
          });

          // The reader for this incoming connection. He receives the messages
          // from the connection and passes them to the main_writer.
          thread::spawn(move || -> Result<()> {
            let mut buf = String::new();
            for c in reader.chars() {
              match c.unwrap_or(std::char::REPLACEMENT_CHARACTER) {
                '\n' => {
                  println!(
                    "{} reader n°{} received '{}'",
                    "debug:".bright_black().bold(),
                    id,
                    buf.yellow()
                  );
                  let sender = writer_send.clone();
                  reader_send
                    .send(Action::ToWriters(buf.clone(), Writer { sender, id }))
                    .chain_err(|| "")?;
                  buf.clear();
                }
                c => buf.push(c),
              }
            }
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
          let mut buf = String::new();
          for c in reader.chars() {
            match c.unwrap_or(REPLACEMENT_CHARACTER) {
              '\n' => {
                println!("{} {}", "remote:".blue().bold(), buf);
                buf.clear()
              }
              c => buf.push(c),
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
    let stderr = &mut std::io::stderr();
    let errmsg = "Error writing to stderr";
    writeln!(stderr, "{} {}", "error:".red().bold(), e).expect(errmsg);
    for e in e.iter().skip(1) {
      writeln!(stderr, "{} {}", "caused by:".bright_black().bold(), e).expect(errmsg);
    }
    // Use `RUST_BACKTRACE=1` to enable the backtraces.
    if let Some(backtrace) = e.backtrace() {
      writeln!(stderr, "{} {:?}", "backtrace:".blue().bold(), backtrace).expect(errmsg);
    }
    std::process::exit(1);
  }
}
