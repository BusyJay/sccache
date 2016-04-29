// Copyright 2016 Mozilla Foundation
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use protobuf::{
    CodedInputStream,
    Message,
    ProtobufError,
    ProtobufResult,
    parse_from_bytes,
};
use protocol::{
    ClientRequest,
    ServerResponse,
};
use retry::retry;
use std::io::{
    self,
    BufReader,
    BufWriter,
    Read,
};
use std::net::TcpStream;

/// A connection to an sccache server.
pub struct ServerConnection {
    /// The socket connected to the server.
    stream : TcpStream,
}

impl ServerConnection {
    /// Send `request` to the server, read and return a `ServerResponse`.
    pub fn request(&mut self, request : ClientRequest)
                   -> ProtobufResult<ServerResponse> {
        trace!("ServerConnection::request");
        let writer = try!(self.stream.try_clone().or_else(|e| Err(ProtobufError::IoError(e))));
        try!(request.write_length_delimited_to_writer(&mut BufWriter::new(writer)));
        trace!("ServerConnection::request: sent request, reading response");
        let reader = try!(self.stream.try_clone().or_else(|e| Err(ProtobufError::IoError(e))));
        let mut buf_read = BufReader::new(reader);
        //FIXME: wish `parse_length_delimited_from` worked here!
        let len = try!({
            let mut is = CodedInputStream::from_buffered_reader(&mut buf_read);
            is.read_raw_varint32()
        });
        trace!("Should read {} more bytes", len);
        let mut buf = vec![0; len as usize];
        try!(buf_read.read_exact(&mut buf).or_else(|e| Err(ProtobufError::IoError(e))));
        trace!("Done reading");
        parse_from_bytes::<ServerResponse>(&mut buf)
    }
}

/// Establish a TCP connection to an sccache server listening on `port`.
pub fn connect_to_server(port : u16) -> io::Result<ServerConnection> {
    let stream = try!(TcpStream::connect(("127.0.0.1", port)));
    Ok(ServerConnection { stream : stream })
}

/// Attempt to establish a TCP connection to an sccache server listening on `port`.
///
/// If the connection fails, retry a few times.
pub fn connect_with_retry(port : u16) -> io::Result<ServerConnection> {
    // TODOs:
    // * Pass the server Child in here, so we can stop retrying
    //   if the process exited.
    // * Send a pipe handle to the server process so it can notify
    //   us once it starts the server instead of us polling.
    match retry(10, 1, || connect_to_server(port), |res| res.is_ok()) {
        Ok(Ok(conn)) => Ok(conn),
        _ => Err(io::Error::new(io::ErrorKind::TimedOut,
                                "Connection to server timed out")),
    }
}
