use std::marker::PhantomData;

// A pipe is a trait for sans-IO components, that represents a bidirectional communication channel.
// To drive them, feed them input from both sides, and poll for output towards both sides.
//
// The pipes decide how to buffer and process these messages.
//                     _____________________
//                    / \                   \
//   FrontInput ---> |   |                   | ---> BackOutput
//                   |   |      P I P E      |
//  FrontOutput <--- |   |                   | <--- BackInput
//                    \_/___________________/
//

trait Pipe<FrontInput, BackInput, BackOutput, FrontOutput> {
    fn handle_front_input(&mut self, message: FrontInput);
    fn handle_back_input(&mut self, message: BackInput);
    fn poll_front_output(&mut self) -> Option<FrontOutput>;
    fn poll_back_output(&mut self) -> Option<BackOutput>;
}

// You can glue two pipes together if their interfaces match, creating in a new pipe.
//                     _____________________ _____________________
//                    / \                   \g\                   \
//   FrontInput ---> |   |                   |l|                   | ---> BackOutput
//                   |   |         A         |u|         B         |
//  FrontOutput <--- |   |                   |e|                   | <--- BackInput
//                    \_/___________________/!/___________________/
//

struct Glue<A, B, FrontInput, BackInput, BToA, BackOutput, FrontOutput, AToB>
where
    A: Pipe<FrontInput, BToA, AToB, FrontOutput>,
    B: Pipe<AToB, BackInput, BackOutput, BToA>,
{
    a: A,
    b: B,
    // This marker is required to be able to use all the generic paramaters.
    _marker: PhantomData<(FrontInput, BackInput, BToA, BackOutput, FrontOutput, AToB)>,
}

impl<A, B, FrontInput, BackInput, BToA, BackOutput, FrontOutput, AToB>
    Glue<A, B, FrontInput, BackInput, BToA, BackOutput, FrontOutput, AToB>
where
    A: Pipe<FrontInput, BToA, AToB, FrontOutput>,
    B: Pipe<AToB, BackInput, BackOutput, BToA>,
{
    fn new(a: A, b: B) -> Self {
        Self {
            a,
            b,
            _marker: PhantomData,
        }
    }
}

// When driving a glued-together pipe, forward messages between the two sub-pipes.
impl<A, B, FrontInput, BackInput, BToA, BackOutput, FrontOutput, AToB>
    Pipe<FrontInput, BackInput, BackOutput, FrontOutput>
    for Glue<A, B, FrontInput, BackInput, BToA, BackOutput, FrontOutput, AToB>
where
    A: Pipe<FrontInput, BToA, AToB, FrontOutput>,
    B: Pipe<AToB, BackInput, BackOutput, BToA>,
{
    fn handle_front_input(&mut self, message: FrontInput) {
        self.a.handle_front_input(message);
    }
    fn handle_back_input(&mut self, message: BackInput) {
        self.b.handle_back_input(message);
    }
    fn poll_back_output(&mut self) -> Option<BackOutput> {
        while let Some(message) = self.a.poll_back_output() {
            self.b.handle_front_input(message);
        }
        self.b.poll_back_output()
    }

    fn poll_front_output(&mut self) -> Option<FrontOutput> {
        while let Some(message) = self.b.poll_front_output() {
            self.a.handle_back_input(message);
        }
        self.a.poll_front_output()
    }
}

#[derive(Default)]
struct BytesToLinesPipe {
    input_from_io: Vec<u8>,
    input_to_io: Vec<String>,
}

impl Pipe<Vec<u8>, String, String, Vec<u8>> for BytesToLinesPipe {
    fn handle_front_input(&mut self, message: Vec<u8>) {
        self.input_from_io.extend(message);
    }
    fn handle_back_input(&mut self, message: String) {
        self.input_to_io.push(message);
    }
    fn poll_back_output(&mut self) -> Option<String> {
        if let Some(pos) = self.input_from_io.iter().position(|&x| x == b'\n') {
            let message = self.input_from_io.drain(..pos).collect();
            self.input_from_io.drain(..1);
            Some(String::from_utf8(message).unwrap())
        } else {
            None
        }
    }
    fn poll_front_output(&mut self) -> Option<Vec<u8>> {
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
    fn handle_front_input(&mut self, message: String) {
        self.input_from_io.push(message);
    }
    fn handle_back_input(&mut self, message: i32) {
        self.output_to_io.push(message.to_string());
    }
    fn poll_back_output(&mut self) -> Option<i32> {
        self.input_from_io.pop().and_then(|message| {
            if let Ok(n) = message.parse() {
                Some(n)
            } else {
                self.output_to_io.push("Invalid number".to_string());
                None
            }
        })
    }
    fn poll_front_output(&mut self) -> Option<String> {
        self.output_to_io.pop()
    }
}

use std::io::{Read, Write};

fn main() {
    let mut bytes_to_numbers_pipe =
        Glue::new(BytesToLinesPipe::default(), StringToNumberPipe::default());
    let mut stdin = std::io::stdin().lock();

    loop {
        if let Some(n) = bytes_to_numbers_pipe.poll_back_output() {
            let n = 2 * n;
            bytes_to_numbers_pipe.handle_back_input(n);
            continue;
        }

        if let Some(bytes) = bytes_to_numbers_pipe.poll_front_output() {
            std::io::stdout().write_all(&bytes).unwrap();
            continue;
        }

        let buf = &mut [0; 100];
        let n = stdin.read(buf).unwrap();
        bytes_to_numbers_pipe.handle_front_input(buf[..n].to_vec());
    }
}
