use std::io::{Error, ErrorKind, Read, Write};
use std::net::TcpListener;
use std::process::exit;
use std::str;

#[derive(Debug)]
struct Request {
    headers: Vec<String>,
    body: String,
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
        let try_read = parse(&mut stream);
        if try_read.is_err() {
            println!("Error while reading from client. {}", try_read.unwrap_err());
            stream.write(b"HTTP/1.0 400 Bad Request\r\nConnection: close\r\n\r\n"); // TODO handle
            continue;
        }
        println!("{:?}", try_read.unwrap());
        stream.write(b"HTTP/1.0 202 Accepted\r\nConnection: close\r\n\r\n"); // TODO handle
    }
    0
}

fn parse(reader: &mut dyn Read) -> std::io::Result<Request> {
    let mut headers: Vec<String> = Vec::new();

    // "global" state
    let mut buf = [0; 32768];
    let max_size = buf.len();
    let mut pos = 0;
    let mut content_length = 0;
    let mut headers_parsed = false;
    let mut header_size = 0;

    // current buffer
    let mut internal_buffer = [0; 512];

    // current parsing state
    let mut parse_pos = 0;
    let mut first_line_skipped = false;

    loop {
        if headers_parsed {
            println!(
                "Debug: conten length: {}, pos: {}, header size: {}",
                content_length, pos, header_size
            );
            let content_left = content_length - (pos - header_size);
            if content_left == 0 {
                break;
            }
        }

        let read = reader.read(&mut internal_buffer)?;
        if read < 1 {
            break;
        }

        if pos + read > max_size {
            let msg: String = format!("Message is too big. Max size {}", max_size);
            return std::io::Result::Err(Error::new(ErrorKind::InvalidData, msg));
        }

        let curr_pos = pos + read;
        buf[pos..curr_pos].copy_from_slice(&internal_buffer[..read]);
        pos = curr_pos;

        //parsing
        if !headers_parsed {
            let orig_parse_pos = parse_pos;
            for (i, byte) in buf[parse_pos..pos].iter().enumerate() {
                if *byte == 13 {
                    if !first_line_skipped {
                        first_line_skipped = true;
                    } else {
                        let try_header = str::from_utf8(&buf[parse_pos + 2..i + orig_parse_pos]);
                        if try_header.is_err() {
                            return std::io::Result::Err(Error::new(
                                ErrorKind::InvalidData,
                                try_header.unwrap_err(),
                            ));
                        }
                        let header = try_header.unwrap();
                        println!("Debug: Found header: {}", header);
                        if header.starts_with("Content-Length: ") {
                            match &header[16..].parse::<usize>() {
                                Ok(cl) => {
                                    println!("Debug: Found content length: {}", cl);
                                    content_length = *cl;
                                }
                                Err(err) => {
                                    println!("{}", &header[16..]);
                                    return std::io::Result::Err(Error::new(
                                        ErrorKind::InvalidData,
                                        format!("{}", err),
                                    ));
                                }
                            }
                            headers.push(header.to_owned());
                        } else if header == "" {
                            let max_body_size = max_size - i - orig_parse_pos;
                            if content_length < 1 || content_length > max_body_size {
                                return std::io::Result::Err(Error::new(
                                    ErrorKind::InvalidData,
                                    format!("Invalid Content-Length {}", content_length),
                                ));
                            }
                            headers_parsed = true;
                            header_size = i + orig_parse_pos + 2;
                        } else {
                            headers.push(header.to_owned());
                        }
                    }
                    parse_pos = i + orig_parse_pos;
                }
            }
        }
    }
    let try_full_request = str::from_utf8(&buf);
    if try_full_request.is_err() {
        return std::io::Result::Err(Error::new(
            ErrorKind::InvalidData,
            format!("{}", try_full_request.unwrap_err()),
        ));
    }
    let req = Request {
        headers,
        body: try_full_request.unwrap()[parse_pos + 2..parse_pos + 2 + content_length].to_owned(),
    };
    Ok(req)
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
