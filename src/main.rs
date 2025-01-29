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

trait Pipe<FrontInput, FrontOutput, BackOutput, BackInput> {
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
    A: Pipe<FrontInput, FrontOutput, AToB, BToA>,
    B: Pipe<AToB, BToA, BackOutput, BackInput>,
{
    a: A,
    b: B,
    // This marker is required to be able to use all the generic paramaters.
    _marker: PhantomData<(FrontInput, BackInput, BToA, BackOutput, FrontOutput, AToB)>,
}

impl<A, B, FrontInput, BackInput, BToA, BackOutput, FrontOutput, AToB>
    Glue<A, B, FrontInput, BackInput, BToA, BackOutput, FrontOutput, AToB>
where
    A: Pipe<FrontInput, FrontOutput, AToB, BToA>,
    B: Pipe<AToB, BToA, BackOutput, BackInput>,
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
    Pipe<FrontInput, FrontOutput, BackOutput, BackInput>
    for Glue<A, B, FrontInput, BackInput, BToA, BackOutput, FrontOutput, AToB>
where
    A: Pipe<FrontInput, FrontOutput, AToB, BToA>,
    B: Pipe<AToB, BToA, BackOutput, BackInput>,
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

///////////////////////////////////////////////////////////////////////////////////////////////////

use std::collections::VecDeque;
use std::io::{Read, Write};

#[derive(Default)]
struct BytesToLinesPipe {
    front_input: VecDeque<u8>,
    back_input: VecDeque<Result<String, String>>,
}

impl Pipe<Vec<u8>, Result<Vec<u8>, Vec<u8>>, String, Result<String, String>> for BytesToLinesPipe {
    fn handle_front_input(&mut self, bytes: Vec<u8>) {
        self.front_input.extend(bytes)
    }
    fn handle_back_input(&mut self, message: Result<String, String>) {
        self.back_input.push_back(message);
    }
    fn poll_back_output(&mut self) -> Option<String> {
        if let Some(pos) = self.front_input.iter().position(|&x| x == b'\n') {
            let message = self.front_input.drain(..pos).collect();
            self.front_input.drain(..1);
            Some(String::from_utf8(message).unwrap())
        } else {
            None
        }
    }
    fn poll_front_output(&mut self) -> Option<Result<Vec<u8>, Vec<u8>>> {
        self.back_input.pop_front().map(|message| {
            message
                .map(|message| {
                    let mut message = message.into_bytes();
                    message.push(b'\n');
                    message
                })
                .map_err(|message| {
                    let mut message = message.into_bytes();
                    message.push(b'\n');
                    message
                })
        })
    }
}

#[derive(Default)]
struct StringsToNumbersPipe {
    front_input: VecDeque<String>,
    front_output: VecDeque<Result<String, String>>,
}

impl Pipe<String, Result<String, String>, i32, i32> for StringsToNumbersPipe {
    fn handle_front_input(&mut self, message: String) {
        self.front_input.push_back(message);
    }
    fn handle_back_input(&mut self, number: i32) {
        self.front_output.push_back(Ok(number.to_string()));
    }
    fn poll_back_output(&mut self) -> Option<i32> {
        self.front_input.pop_front().and_then(|message| {
            if let Ok(n) = message.parse() {
                Some(n)
            } else {
                self.front_output
                    .push_back(Err(format!("Invalid number: {:?}", message)));
                None
            }
        })
    }
    fn poll_front_output(&mut self) -> Option<Result<String, String>> {
        self.front_output.pop_front()
    }
}

fn main() {
    let mut bytes_to_numbers_pipe =
        Glue::new(BytesToLinesPipe::default(), StringsToNumbersPipe::default());

    let mut stdin = std::io::stdin().lock();
    loop {
        if let Some(n) = bytes_to_numbers_pipe.poll_back_output() {
            let n = 2 * n;
            bytes_to_numbers_pipe.handle_back_input(n);
            continue;
        }

        match bytes_to_numbers_pipe.poll_front_output() {
            Some(Ok(bytes)) => {
                std::io::stdout().write_all(&bytes).unwrap();
                continue;
            }
            Some(Err(bytes)) => {
                std::io::stderr().write_all(&bytes).unwrap();
                continue;
            }
            None => (),
        }

        let buf = &mut [0; 100];
        let n = stdin.read(buf).unwrap();
        bytes_to_numbers_pipe.handle_front_input(buf[..n].to_vec());
    }
}
