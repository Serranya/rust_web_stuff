use std::fmt;
use std::io::{Error, ErrorKind, Read, Write};
use std::net::TcpListener;
use std::process::exit;

const LF: u8 = b'\n';
const CR: u8 = b'\r';
const SP: u8 = b' ';
const HT: u8 = b'\t';
const COLON: u8 = b':';

struct Request<'a> {
    line: RequestLine,
    headers: Vec<HeaderField>,
    buf: &'a [u8],
}

#[derive(Debug)]
struct RequestLine {
    method: (usize, usize),
    uri: (usize, usize),
    version: (usize, usize),
}

impl fmt::Display for Request<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "Method: {}",
            buf_str(self.buf, self.line.method.0, self.line.method.1)
        )?;
        writeln!(
            f,
            "URI: {}",
            buf_str(self.buf, self.line.uri.0, self.line.uri.1)
        )?;
        writeln!(
            f,
            "Version: {}",
            buf_str(self.buf, self.line.version.0, self.line.version.1)
        )?;
        if self.headers.is_empty() {
            write!(f, "No headers")?;
        } else {
            write!(f, "Headers:")?;
            for header in &self.headers {
                writeln!(
                    f,
                    "{}: {}",
                    buf_str(self.buf, header.name.0, header.name.1),
                    buf_str(self.buf, header.value.0, header.value.1)
                )?;
            }
        }
        write!(f, "TODO")
    }
}

#[derive(Debug)]
struct HeaderField {
    name: (usize, usize),
    value: (usize, usize),
}

struct MyReader<'a> {
    /// does the actual reading
    inner: &'a mut dyn Read,
    /// how much of this reader was consumed
    pos: usize,
    /// we read into this
    buf: Box<[u8]>,
    /// how much of `buf` is filled
    buf_pos: usize,
}

impl MyReader<'_> {
    fn new(inner: & mut dyn Read) -> MyReader {
        MyReader {
            inner,
            pos: 0,
            buf: vec![0; 32768].into_boxed_slice(),
            buf_pos: 0,
        }
    }

    fn push_back(&mut self) {
        if self.pos == 0 {
            panic!("Can't push back past 0.");
        }
        self.pos -= 1;
    }

    fn push_back_by(&mut self, amount: usize) {
        if self.pos < amount {
            panic!("Cant push back by {}. Only {} left.", amount, self.pos);
        }
        self.pos -= amount;
    }

    fn into_content(self) -> Box<[u8]> {
        self.buf
    }

    fn borrow_content(&self) -> &[u8] {
        &self.buf
    }
}

impl Iterator for MyReader<'_> {
    type Item = Result<u8, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let ret;
        if self.pos < self.buf_pos {
            ret = Some(Ok(self.buf[self.pos]));
        } else if self.buf_pos < self.buf.len() {
            let read = self.inner.read(&mut self.buf[self.buf_pos..]);
            match read {
                Ok(val) if val == 0 => {
                    return None;
                }
                Ok(val) => {
                    self.buf_pos += val;
                }
                Err(e) => {
                    return Some(Err(e));
                }
            }
            ret = Some(Ok(self.buf[self.pos]));
        } else {
            return None;
        }
        self.pos += 1; // TODO check for overflow or max size
        ret
    }
}

fn main() {
    let exit_status = run();
    exit(exit_status);
}

fn run() -> i32 {
    let listener = match TcpListener::bind(":::8338") {
        Ok(listener) => listener,
        Err(error) => {
            println!("Could not bind. {}", error);
            return 1;
        }
    };
    for try_stream in listener.incoming() {
        match &try_stream {
            Ok(_) => println!("New client!"),
            Err(error) => println!("Error while accepting client. {}", error),
        }
        if try_stream.is_err() {
            continue;
        }
        let mut stream = try_stream.unwrap();
        let mut reader = MyReader::new(&mut stream);
        let parsed = parse(&mut reader);

        if parsed.is_err() {
            println!("Error while reading from client.");
            stream.write(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n"); // TODO handle
            continue;
        }
        println!("{}", parsed.unwrap());
        stream.write(b"HTTP/1.1 202 Accepted\r\nConnection: close\r\n\r\n"); // TODO handle
    }
    0
}

fn parse<'a>(bytes: &'a mut MyReader) -> Result<Request<'a>, Error> {
    let req_line = parse_request_line(bytes)?;
    println!("{:?}", req_line);

    let headers = parse_headers(bytes, req_line.version.1)?;
    println!("{:?}", headers);

    // TODO skipp LWS

    // TODO if post read body
    // respect size header.

    Ok(Request {
        line: req_line,
        headers,
        buf: bytes.borrow_content(),
    })
}

fn parse_request_line(bytes: &mut MyReader) -> Result<RequestLine, Error> {
    let method = read_token(bytes, 0)?;
    let chomped = chomp_whitespace(bytes)?;
    let uri = read_token(bytes, method.1 + chomped)?;
    let chomped = chomp_whitespace(bytes)?;
    let version = read_token(bytes, uri.1 + chomped)?;
    Ok(RequestLine {
        method,
        uri,
        version,
    })
}

fn parse_headers(bytes: &mut MyReader, start: usize) -> Result<Vec<HeaderField>, Error> {
    let mut headers = Vec::new();
    let mut start = start;
    loop {
        let chomped = chomp_whitespace_safe(bytes, 4)?;
        println!("Chomped: {}", chomped);
        if chomped > 2 {
            bytes.push_back_by(chomped);
            return Ok(headers);
        }
        let header = parse_header(bytes, start + chomped)?;
        start = header.value.1;
        println!("Found header: {:?}", header);
        headers.push(header);
    }
}

fn parse_header(bytes: &mut MyReader, start: usize) -> Result<HeaderField, Error> {
    let name = read_header_name(bytes, start)?;
    bytes.next(); // skip colon
    let value = read_header_value(bytes, name.1 + 1)?;
    Ok(HeaderField { name, value })
}

fn read_header_name(bytes: &mut MyReader, start: usize) -> Result<(usize, usize), Error> {
    for (i, byte) in bytes.enumerate() {
        let byte = byte?;
        if byte == COLON {
            bytes.push_back();
            return Ok((start, start + i));
        }
    }
    Err(Error::new(
        ErrorKind::UnexpectedEof,
        "Encountered end of file while looking for header name",
    ))
}

// does not handle line continuations yet..
fn read_header_value(bytes: &mut MyReader, start: usize) -> Result<(usize, usize), Error> {
    for (i, byte) in bytes.enumerate() {
        let byte = byte?;
        if byte == CR {
            let chomped = try_chomp_lws(bytes)?;
            if chomped.is_some() {
                panic!("Unhandeled line continuation");
            }
            bytes.push_back();
            return Ok((start, start + i));
        }
    }

    Err(Error::new(
        ErrorKind::UnexpectedEof,
        "Encountered end of file while looking for header value",
    ))
}

fn read_token(bytes: &mut MyReader, start: usize) -> Result<(usize, usize), Error> {
    for (i, byte) in bytes.enumerate() {
        let byte = byte?;
        if byte == SP || byte == CR {
            bytes.push_back();
            return Ok((start, start + i));
        }
    }
    Err(Error::new(
        ErrorKind::UnexpectedEof,
        "Encountered end of file while looking for token",
    ))
}

fn chomp_whitespace(bytes: &mut MyReader) -> Result<usize, Error> {
    chomp_whitespace_safe(bytes, usize::max_value())
}

fn chomp_whitespace_safe(bytes: &mut MyReader, max: usize) -> Result<usize, Error> {
    for (i, byte) in bytes.enumerate() {
        let byte = byte?;
        if !(byte == SP || byte == CR || byte == LF) {
            bytes.push_back();
            return Ok(i);
        }
        println!("i is {}", i);
        if i == max - 1 {
            return Ok(i);
        }
    }
    Err(Error::new(
        ErrorKind::UnexpectedEof,
        "Encountered end of file while looking for token",
    ))
}

fn try_chomp_lws(bytes: &mut MyReader) -> Result<Option<usize>, Error> {
    let byte = bytes.next();
    match byte.transpose()? {
        Some(b) if b == CR => {}
        Some(_) => {
            bytes.push_back();
            return Ok(None);
        }
        None => return Ok(None),
    }

    let byte = bytes.next();
    match byte.transpose()? {
        Some(b) if b == LF => {}
        Some(_) => {
            bytes.push_back_by(2);
            return Ok(None);
        }
        None => return Ok(None),
    }

    let byte = bytes.next();
    match byte.transpose()? {
        Some(b) if b == SP || b == HT => {}
        Some(_) => {
            bytes.push_back_by(3);
            return Ok(None);
        }
        None => return Ok(None),
    }

    let mut i = 0;
    loop {
        let byte = bytes.next().transpose()?;
        if byte.is_none() {
            break;
        }

        let byte = byte.unwrap();
        if !(byte == SP || byte == HT) {
            bytes.push_back();
            return Ok(Some(3 + i));
        }
        i += 1;
    }

    Ok(Some(3 + i))
}

fn buf_str(buf: &[u8], start: usize, end: usize) -> String {
    String::from_utf8(buf[start..end].to_vec()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Result;

    struct FailReader {}

    impl Read for FailReader {
        fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
            return std::io::Result::Err(Error::new(ErrorKind::InvalidData, "not implemented"));
        }
    }

    #[test]
    fn parse_works() {
        assert!(parse(&mut FailReader {}).is_err());
    }
}
