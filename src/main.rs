use std::marker::PhantomData;

// A pipe is a "sans-IO" bidirectional communication channel.
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
// As an example, let's build two pipes.

use std::collections::VecDeque;
use std::io::{Read, Write};

// The first pipe translates between a stream of bytes and a stream of lines.
// In the backwards direction, we differentiate between "ok" and "error" messages.
// That way, this pipe can be attached to stdin, stdout and stderr.

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
        let into_bytes = |message: String| {
            let mut message = message.into_bytes();
            message.push(b'\n');
            message
        };
        self.back_input
            .pop_front()
            .map(|message| message.map(into_bytes).map_err(into_bytes))
    }
}

// The second pipe converts string to numbers, and the other way around.
// When fed a string that's not a number, it returns an error.

#[derive(Default)]
struct StringsToNumbersPipe {
    back_output: VecDeque<i32>,
    front_output: VecDeque<Result<String, String>>,
}

impl Pipe<String, Result<String, String>, i32, i32> for StringsToNumbersPipe {
    fn handle_front_input(&mut self, message: String) {
        if let Ok(n) = message.parse() {
            self.back_output.push_back(n);
        } else {
            self.front_output
                .push_back(Err(format!("Invalid number: {:?}", message)));
        }
    }
    fn handle_back_input(&mut self, number: i32) {
        self.front_output.push_back(Ok(number.to_string()));
    }
    fn poll_back_output(&mut self) -> Option<i32> {
        self.back_output.pop_front()
    }
    fn poll_front_output(&mut self) -> Option<Result<String, String>> {
        self.front_output.pop_front()
    }
}

// We can now glue these pipes together to create a program that reads numbers from stdin,
// processes the numbers, and sends the results to stdout. Here's a synchronous version.

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

// If we wanted, we could drive the same pipe asynchronously, by using async/await and Tokio.
// That way, it would be easy do drive more than one pipe at the same time, by tokio::select!-ing
// multiple event sources.

/*
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() {
    let mut bytes_to_numbers_pipe =
        Glue::new(BytesToLinesPipe::default(), StringsToNumbersPipe::default());

    let mut stdin = tokio::io::stdin();
    loop {
        if let Some(n) = bytes_to_numbers_pipe.poll_back_output() {
            let n = 2 * n;
            bytes_to_numbers_pipe.handle_back_input(n);
            continue;
        }

        match bytes_to_numbers_pipe.poll_front_output() {
            Some(Ok(bytes)) => {
                tokio::io::stdout().write_all(&bytes).await.unwrap();
                continue;
            }
            Some(Err(bytes)) => {
                tokio::io::stderr().write_all(&bytes).await.unwrap();
                continue;
            }
            None => (),
        }

        let mut buf = vec![0; 100];
        let n = stdin.read(&mut buf).await.unwrap();
        bytes_to_numbers_pipe.handle_front_input(buf[..n].to_vec());
    }
}
*/

///////////////////////////////////////////////////////////////////////////////////////////////////
// Tests are modular and fast, because they don't actually have to do IO.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_to_lines_pipe() {
        let mut pipe = BytesToLinesPipe::default();

        pipe.handle_front_input(b"hello\nworld".to_vec());
        assert_eq!(pipe.poll_back_output(), Some("hello".to_string()));
        assert_eq!(pipe.poll_back_output(), None);

        pipe.handle_front_input(b"\n".to_vec());
        assert_eq!(pipe.poll_back_output(), Some("world".to_string()));

        pipe.handle_back_input(Ok("hello".to_string()));
        assert_eq!(pipe.poll_front_output(), Some(Ok(b"hello\n".to_vec())));
        assert_eq!(pipe.poll_front_output(), None);

        pipe.handle_back_input(Err("hello".to_string()));
        assert_eq!(pipe.poll_front_output(), Some(Err(b"hello\n".to_vec())));
        assert_eq!(pipe.poll_front_output(), None);
    }

    #[test]
    fn test_strings_to_numbers_pipe() {
        let mut pipe = StringsToNumbersPipe::default();

        pipe.handle_front_input("42".to_string());
        assert_eq!(pipe.poll_back_output(), Some(42));
        assert_eq!(pipe.poll_back_output(), None);

        pipe.handle_front_input("hello".to_string());
        // An error at the front.
        let front_output = pipe.poll_front_output();
        assert!(front_output.is_some());
        assert!(front_output.unwrap().is_err());
        // No output at the back.
        let back_output = pipe.poll_back_output();
        assert!(back_output.is_none());
    }
}
