use std::net::TcpStream;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::io::Write;

use console::Term;

fn main() -> std::io::Result<()> {
    println!("Connecting...");
    let mut stream = TcpStream::connect("192.168.1.37:1380")?;
    println!("Connected");
    let key = Arc::new(AtomicU8::new(0));

    {
        let key = key.clone();
        std::thread::spawn(move || loop {
            let term = Term::stdout();
            let c = term.read_char().unwrap();
            key.store(c as u8, Ordering::SeqCst);
        });
    }

    let mut speed = 3;

    loop {
        let sf_left = 100;
        let sf_right = 50;

        let c = key.swap(0, Ordering::SeqCst);
        let (l, r) = match c as char {
            'w' => (100, 100),
            's' => (-100, -100),
            'a' => (-100, 100),
            'd' => (100, -100),
            'q' => (50, 100),
            'e' => (100, 50),
            'o' => {
                speed = std::cmp::min(speed + 1, 5);
                (0,0)
            }
            'l' => {
                speed = std::cmp::max(speed - 1, 1);
                (0,0)
            }
            _ => (0, 0),
        };
        writeln!(stream, "{} {}", l * sf_left / 100 * speed / 5, r * sf_right / 100 * speed / 5)?;
        std::thread::sleep(Duration::from_millis(100));
    }
}
