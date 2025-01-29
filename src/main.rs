use std::io::{Read, Write};
use std::marker::PhantomData;

// A pipe is a trait for sans-IO components, that represents a bidirectional communication channel.
// To drive them, feed them input from both sides, and poll for output towards both sides.
// The pipes decide how to buffer and process these messages.
//                   _____________________
//                  / \                   \
// InputFromIO --> |   |                   | --> OutputFromIO
//                 |   |      P I P E      |
//  OutputToIO <-- |   |                   | <-- InputToIO
//                  \_/___________________/
//

trait Pipe<InputFromIO, InputToIO, OutputFromIO, OutputToIO> {
    fn handle_input_from_io(&mut self, message: InputFromIO);
    fn handle_input_to_io(&mut self, message: InputToIO);
    fn poll_transmit_from_io(&mut self) -> Option<OutputFromIO>;
    fn poll_transmit_to_io(&mut self) -> Option<OutputToIO>;
}

//                   _____________________ _____________________
//                  / \                   \g\                   \
// InputFromIO --> |   |                   |l|                   | --> OutputFromIO
//                 |   |         A         |u|         B         |
//  OutputToIO <-- |   |                   |e|                   | <-- InputToIO
//                  \_/___________________/!/___________________/
//

struct Glue<A, B, InputFromIO, InputToIO, InputToA, OutputFromIO, OutputToIO, OutputFromA>
where
    A: Pipe<InputFromIO, InputToA, OutputFromA, OutputToIO>,
    B: Pipe<OutputFromA, InputToIO, OutputFromIO, InputToA>,
{
    a: A,
    b: B,
    _marker: PhantomData<(
        InputFromIO,
        InputToIO,
        InputToA,
        OutputFromIO,
        OutputToIO,
        OutputFromA,
    )>,
}

impl<A, B, InputFromIO, InputToIO, InputToA, OutputFromIO, OutputToIO, OutputFromA>
    Glue<A, B, InputFromIO, InputToIO, InputToA, OutputFromIO, OutputToIO, OutputFromA>
where
    A: Pipe<InputFromIO, InputToA, OutputFromA, OutputToIO>,
    B: Pipe<OutputFromA, InputToIO, OutputFromIO, InputToA>,
{
    fn new(a: A, b: B) -> Self {
        Self {
            a,
            b,
            _marker: PhantomData,
        }
    }
}

impl<A, B, InputFromIO, InputToIO, InputToA, OutputFromIO, OutputToIO, OutputFromA>
    Pipe<InputFromIO, InputToIO, OutputFromIO, OutputToIO>
    for Glue<A, B, InputFromIO, InputToIO, InputToA, OutputFromIO, OutputToIO, OutputFromA>
where
    A: Pipe<InputFromIO, InputToA, OutputFromA, OutputToIO>,
    B: Pipe<OutputFromA, InputToIO, OutputFromIO, InputToA>,
{
    fn handle_input_from_io(&mut self, message: InputFromIO) {
        self.a.handle_input_from_io(message);
    }
    fn handle_input_to_io(&mut self, message: InputToIO) {
        self.b.handle_input_to_io(message);
    }
    fn poll_transmit_from_io(&mut self) -> Option<OutputFromIO> {
        while let Some(message) = self.a.poll_transmit_from_io() {
            self.b.handle_input_from_io(message);
        }
        self.b.poll_transmit_from_io()
    }

    fn poll_transmit_to_io(&mut self) -> Option<OutputToIO> {
        while let Some(message) = self.b.poll_transmit_to_io() {
            self.a.handle_input_to_io(message);
        }
        self.a.poll_transmit_to_io()
    }
}

//////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Default)]
struct BytesToLinesPipe {
    input_from_io: Vec<u8>,
    input_to_io: Vec<String>,
}

impl Pipe<Vec<u8>, String, String, Vec<u8>> for BytesToLinesPipe {
    fn handle_input_from_io(&mut self, message: Vec<u8>) {
        self.input_from_io.extend(message);
    }
    fn handle_input_to_io(&mut self, message: String) {
        self.input_to_io.push(message);
    }
    fn poll_transmit_from_io(&mut self) -> Option<String> {
        if let Some(pos) = self.input_from_io.iter().position(|&x| x == b'\n') {
            let message = self.input_from_io.drain(..pos).collect();
            self.input_from_io.drain(..1);
            Some(String::from_utf8(message).unwrap())
        } else {
            None
        }
    }
    fn poll_transmit_to_io(&mut self) -> Option<Vec<u8>> {
        self.input_to_io.pop().map(|message| {
            let mut bytes = message.into_bytes();
            // append new line
            bytes.push(b'\n');
            bytes
        })
    }
}

#[derive(Default)]
struct StringToNumberPipe {
    input_from_io: Vec<String>,
    output_to_io: Vec<String>,
}

impl Pipe<String, i32, i32, String> for StringToNumberPipe {
    fn handle_input_from_io(&mut self, message: String) {
        self.input_from_io.push(message);
    }
    fn handle_input_to_io(&mut self, message: i32) {
        self.output_to_io.push(message.to_string());
    }
    fn poll_transmit_from_io(&mut self) -> Option<i32> {
        self.input_from_io.pop().and_then(|message| {
            if let Ok(n) = message.parse() {
                Some(n)
            } else {
                self.output_to_io.push("Invalid number".to_string());
                None
            }
        })
    }
    fn poll_transmit_to_io(&mut self) -> Option<String> {
        self.output_to_io.pop()
    }
}

fn main() {
    let mut bytes_to_numbers_pipe =
        Glue::new(BytesToLinesPipe::default(), StringToNumberPipe::default());
    let mut stdin = std::io::stdin().lock();

    loop {
        if let Some(n) = bytes_to_numbers_pipe.poll_transmit_from_io() {
            let n = 2 * n;
            bytes_to_numbers_pipe.handle_input_to_io(n);
            continue;
        }

        if let Some(bytes) = bytes_to_numbers_pipe.poll_transmit_to_io() {
            std::io::stdout().write_all(&bytes).unwrap();
            continue;
        }

        let buf = &mut [0; 100];
        let n = stdin.read(buf).unwrap();
        bytes_to_numbers_pipe.handle_input_from_io(buf[..n].to_vec());
    }
}
