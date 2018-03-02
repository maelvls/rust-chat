use std::io::Write;
use std::io::BufRead;
use std::io::BufReader;
use std::net::TcpListener;
use std::thread;

// TODO: gérer proprement toutes ces putain d'exceptions de merde !

fn main() {
    let listener = TcpListener::bind("127.0.0.1:9123").unwrap();
    println!("listening started, ready to accept");
    for stream in listener.incoming() {
        let write_stream = stream.unwrap();
        let read_stream = write_stream.try_clone().unwrap_or_else(|err| {
            panic!("Impossible de dupliquer le stream: {:}", err);
        });
        thread::spawn(|| {
            let mut stream = write_stream;
            stream.write(b"Hello World\r\n").unwrap();
        });
        thread::spawn(|| {
            let mut line = String::new();
            let mut reader = BufReader::new(read_stream);
            reader.read_line(&mut line).unwrap();
            println!("{}", line.trim());
        });
    }
}
