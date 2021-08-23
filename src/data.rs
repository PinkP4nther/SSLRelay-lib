use std::time::Duration;
use openssl::ssl::{SslConnector, SslMethod, SslStream, SslVerifyMode};
use std::net::TcpStream;
use std::io::{self, Read, Write};

#[derive(PartialEq)]
pub enum StreamDirection {
    Upstream,// Data coming from remote host
    DownStream,// Data coming from origin host
}

pub struct DataHandler {
    pub tcp_stream: Option<SslStream<TcpStream>>,
    relay_stream: Option<SslStream<TcpStream>>,
    remote_host: String,
    pub stream_direction: StreamDirection,
}

impl DataHandler {

    pub fn new(tcp_stream: SslStream<TcpStream>, remote_host: String) -> Self {
        let _ = tcp_stream.get_ref().set_read_timeout(Some(Duration::from_millis(100)));
        DataHandler {
            tcp_stream: Some(tcp_stream),
            relay_stream: None,
            remote_host,
            stream_direction: StreamDirection::DownStream,
        }
    }

    pub fn get_data_stream(&mut self, data: &mut Vec<u8>) -> usize {

        let mut data_length: usize = 0;

        match self.stream_direction {

            StreamDirection::DownStream => {

                loop {

                    let mut r_buf = [0; 1024];

                    match self.tcp_stream.as_mut().unwrap().read(&mut r_buf) {

                        Ok(bytes_read) => {

                            if bytes_read == 0 {
                                break;

                            } else if bytes_read != 0 && bytes_read <= 1024 {

                                let mut tmp_buf = r_buf.to_vec();
                                tmp_buf.truncate(bytes_read);

                                let _bw = data.write(&tmp_buf).unwrap();
                                data_length += bytes_read;

                            } else {
                                println!("[+] Else hit!!!!!!!!!!!!!!!!!!!!!!");
                            }
                        },
                        Err(e) => {
                            match e.kind() {
                                io::ErrorKind::WouldBlock => {
                                    break;
                                },
                                _ => {println!("[!!!] Got error: {}",e);}
                            }
                        },
                    }
                }
            },
            StreamDirection::Upstream => {
                loop {

                    let mut r_buf = [0; 1024];

                    match self.relay_stream.as_mut().unwrap().read(&mut r_buf) {

                        Ok(bytes_read) => {

                            if bytes_read == 0 {
                                break;

                            } else if bytes_read != 0 && bytes_read <= 1024 {

                                let mut tmp_buf = r_buf.to_vec();
                                tmp_buf.truncate(bytes_read);

                                let _bw = data.write(&tmp_buf).unwrap();
                                data_length += bytes_read;

                            } else {
                                println!("[+] Else hit!!!!!!!!!!!!!!!!!!!!!!");
                            }
                        },
                        Err(e) => {
                            match e.kind() {
                                io::ErrorKind::WouldBlock => {
                                    break;
                                },
                                _ => {println!("[!!!] Got error: {}",e);}
                            }
                        },
                    }
                }
            }
        }
        return data_length;
    }

    pub fn relay_data(&mut self, data: &Vec<u8>) -> Option<i8> {

        let mut retries = 3;
        loop {

            let mut sslbuild = SslConnector::builder(SslMethod::tls()).unwrap();
            sslbuild.set_verify(SslVerifyMode::NONE);
            let connector = sslbuild.build();
            let stream = TcpStream::connect(&self.remote_host).unwrap();
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut stream = match connector.connect(&self.remote_host, stream) {
                Ok(s) => s,
                Err(e) => {
                    println!("[Error] {}", e);
                    if retries == 0 {
                        println!("[!] Request relay retries: 0");
                        return None;
                    }
                    retries -= 1;
                    continue;
                }
            };

            stream.write_all(&data).unwrap();
            let _ = stream.flush();
            self.relay_stream = Some(stream);
            return Some(0);
        }
    }
} // DataHandler