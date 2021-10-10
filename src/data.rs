use crate::{
    DownStreamInner,
    UpStreamInner,
    FullDuplexTcpState,
    DataPipe,
    DataStreamType,
    Sender,
    Receiver,
    Shutdown,
    mpsc,
    Duration,
    Read,
    Write,
    io,
};

impl DownStreamInner {

    fn handle_error(error_description: &str) {
        println!("[SSLRelay DownStream Thread Error]: {}", error_description);
    }

    pub fn ds_handler(self, data_out: Sender<FullDuplexTcpState>, data_in: Receiver<DataPipe>) {

        match &self.ds_stream {
            DataStreamType::RAW(_) => self.handle_raw(data_out, data_in),
            DataStreamType::TLS(_) => self.handle_tls(data_out, data_in),
        }
    }

    fn handle_raw(mut self, data_out: Sender<FullDuplexTcpState>, data_in: Receiver<DataPipe>) {

        let mut raw_stream = match &self.ds_stream {
            DataStreamType::RAW(ref s) => s,
            _ => return,
        };

        loop {

            match data_in.recv_timeout(Duration::from_millis(50)) {

                // DataPipe Received
                Ok(data_received) => {

                    match data_received {
                        DataPipe::DataWrite(data) => {

                            match raw_stream.write_all(&data) {
                                Ok(()) => {},
                                Err(_e) => {
                                    Self::handle_error("Failed to write data to DownStream tcp stream!");
                                    let _ = data_out.send(FullDuplexTcpState::DownStreamShutDown);
                                    let _ = raw_stream.shutdown(Shutdown::Both);
                                    return;
                                }
                            }
                            let _ = raw_stream.flush();

                        },
                        DataPipe::Shutdown => {
                            let _ = raw_stream.shutdown(Shutdown::Both);
                            return;
                        },
                    }
                },
                Err(_e) => {
                    match _e {
                        mpsc::RecvTimeoutError::Timeout => {},
                        mpsc::RecvTimeoutError::Disconnected => {
                            Self::handle_error("DownStream data_in channel is disconnected!");
                            return;
                        }
                    }
                }
            }// End of data_in receive

            // If received data
            if let Some(byte_count) = Self::get_data_stream(&mut raw_stream, &mut self.internal_data_buffer) {
                if byte_count > 0 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::UpStreamWrite(self.internal_data_buffer.clone())) {
                        
                        //Self::handle_error(format!("Failed to send UpStreamWrite to main thread: {}", e).as_str());
                        let _ = raw_stream.shutdown(Shutdown::Both);
                        return;
                    }

                    self.internal_data_buffer.clear();

                } else if byte_count == 0 || byte_count == -2 {

                    let _ = data_out.send(FullDuplexTcpState::DownStreamShutDown);
                    let _ = raw_stream.shutdown(Shutdown::Both);
                    return;

                } else if byte_count == -1 {
                    continue;
                }
            } else {

            }
        }
    }

    fn handle_tls(mut self, data_out: Sender<FullDuplexTcpState>, data_in: Receiver<DataPipe>) {

        let mut tls_stream = match self.ds_stream {
            DataStreamType::TLS(ref mut s) => s,
            _ => return,
        };

        loop {

            match data_in.recv_timeout(Duration::from_millis(50)) {

                // DataPipe Received
                Ok(data_received) => {

                    match data_received {
                        DataPipe::DataWrite(data) => {

                            match tls_stream.write_all(&data) {
                                Ok(()) => {},
                                Err(_e) => {
                                    Self::handle_error("Failed to write data to DownStream tcp stream!");
                                    let _ = data_out.send(FullDuplexTcpState::DownStreamShutDown);
                                    let _ = tls_stream.shutdown();
                                    return;
                                }
                            }
                            let _ = tls_stream.flush();
                        },
                        DataPipe::Shutdown => {
                            let _ = tls_stream.shutdown();
                            return;
                        },
                    }
                },
                Err(_e) => {
                    match _e {
                        mpsc::RecvTimeoutError::Timeout => {},
                        mpsc::RecvTimeoutError::Disconnected => {
                            Self::handle_error("DownStream data_in channel is disconnected!");
                            return;
                        }
                    }
                }
            }// End of data_in receive

            // If received data
            if let Some(byte_count) = Self::get_data_stream(&mut tls_stream, &mut self.internal_data_buffer) {
                if byte_count > 0 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::UpStreamWrite(self.internal_data_buffer.clone())) {
                        
                        //Self::handle_error(format!("Failed to send UpStreamWrite to main thread: {}", e).as_str());
                        let _ = tls_stream.shutdown();
                        return;
                    }

                    self.internal_data_buffer.clear();

                } else if byte_count == 0 || byte_count == -2 {

                    let _ = data_out.send(FullDuplexTcpState::DownStreamShutDown);
                    let _ = tls_stream.shutdown();
                    return;

                } else if byte_count == -1 {
                    continue;
                }
            } else {

            }
        }
    }

    fn get_data_stream<S: Read>(stream: &mut S, internal_data_buffer: &mut Vec<u8>) -> Option<i64> {

        let mut data_length: i64 = 0;

        loop {

            let mut r_buf = [0; 1024];

            match stream.read(&mut r_buf) {

                Ok(bytes_read) => {

                    if bytes_read == 0 {
                        break;

                    } else if bytes_read != 0 && bytes_read <= 1024 {

                        /*
                        let mut tmp_buf = r_buf.to_vec();
                        tmp_buf.truncate(bytes_read);
                        */
                        

                        //let _bw = self.internal_data_buffer.write(&tmp_buf).unwrap();

                        let _bw = internal_data_buffer.write(r_buf.split_at(bytes_read).0).unwrap();
                        data_length += bytes_read as i64;

                    } else {
                        println!("[+] Else hit!!!!!!!!!!!!!!!!!!!!!!");
                    }
                },
                Err(e) => {
                    match e.kind() {

                        io::ErrorKind::WouldBlock => {
                            if data_length == 0 {

                                data_length = -1;
                            }
                            break;

                        },
                        io::ErrorKind::ConnectionReset => {
                            data_length = -2;
                            break;
                        },
                        _ => {println!("[!!!] Got error: {}",e);}
                    }
                },
            }
        }
        return Some(data_length);
    }
}


impl UpStreamInner {

    fn handle_error(error_description: &str) {
        println!("[SSLRelay UpStream Thread Error]: {}", error_description);
    }

    pub fn us_handler(self, data_out: Sender<FullDuplexTcpState>, data_in: Receiver<DataPipe>) {

        match &self.us_stream {
            DataStreamType::RAW(_) => self.handle_raw(data_out, data_in),
            DataStreamType::TLS(_) => self.handle_tls(data_out, data_in),
        }

    }

    fn handle_raw(mut self, data_out: Sender<FullDuplexTcpState>, data_in: Receiver<DataPipe>) {

        let mut raw_stream = match self.us_stream {
            DataStreamType::RAW(ref s) => s,
            _ => return,
        };

        loop {

            match data_in.recv_timeout(Duration::from_millis(50)) {

                Ok(data_received) => {

                    match data_received {
                        DataPipe::DataWrite(data) => {

                            match raw_stream.write_all(&data) {
                                Ok(()) => {},
                                Err(_e) => {
                                    Self::handle_error("Failed to write data to UpStream tcp stream!");
                                    let _ = data_out.send(FullDuplexTcpState::UpStreamShutDown);
                                    let _ = raw_stream.shutdown(Shutdown::Both);
                                    return;
                                }
                            }
                            let _ = raw_stream.flush();
                        },
                        DataPipe::Shutdown => {
                            let _ = raw_stream.shutdown(Shutdown::Both);
                            return;
                        }
                    }
                },
                Err(e) => {
                    match e {
                        mpsc::RecvTimeoutError::Timeout => {},
                        mpsc::RecvTimeoutError::Disconnected => {
                            Self::handle_error("UpStream data_in channel is disconnected!");
                            return;
                        }
                    }
                }
            }// End of data_in receive

            if let Some(byte_count) = Self::get_data_stream(&mut raw_stream, &mut self.internal_data_buffer) {
                if byte_count > 0 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::DownStreamWrite(self.internal_data_buffer.clone())) {
                        
                        //Self::handle_error(format!("Failed to send DownStreamWrite to main thread: {}", e).as_str());
                        let _ = raw_stream.shutdown(Shutdown::Both);
                        return;
                    }

                    self.internal_data_buffer.clear();

                } else if byte_count == 0 || byte_count == -2 {

                    let _ = data_out.send(FullDuplexTcpState::UpStreamShutDown);
                    let _ = raw_stream.shutdown(Shutdown::Both);
                    return;
                } else if byte_count == -1 {
                    continue;
                }
            } else {
            }
        }
    }

    fn handle_tls(mut self, data_out: Sender<FullDuplexTcpState>, data_in: Receiver<DataPipe>) {

        let mut tls_stream = match self.us_stream {
            DataStreamType::TLS(ref mut s) => s,
            _ => return,
        };

        loop {

            match data_in.recv_timeout(Duration::from_millis(50)) {

                Ok(data_received) => {

                    match data_received {
                        DataPipe::DataWrite(data) => {

                            match tls_stream.write_all(&data) {
                                Ok(()) => {},
                                Err(_e) => {
                                    Self::handle_error("Failed to write data to UpStream tcp stream!");
                                    let _ = data_out.send(FullDuplexTcpState::UpStreamShutDown);
                                    let _ = tls_stream.shutdown();
                                    return;
                                }
                            }
                            let _ = tls_stream.flush();
                        },
                        DataPipe::Shutdown => {
                            let _ = tls_stream.shutdown();
                            return;
                        }
                    }
                },
                Err(e) => {
                    match e {
                        mpsc::RecvTimeoutError::Timeout => {},
                        mpsc::RecvTimeoutError::Disconnected => {
                            Self::handle_error("UpStream data_in channel is disconnected!");
                            return;
                        }
                    }
                }
            }// End of data_in receive

            if let Some(byte_count) = Self::get_data_stream(&mut tls_stream, &mut self.internal_data_buffer) {
                if byte_count > 0 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::DownStreamWrite(self.internal_data_buffer.clone())) {

                        //Self::handle_error(format!("Failed to send DownStreamWrite to main thread: {}", e).as_str());
                        let _ = tls_stream.shutdown();
                        return;
                    }

                    self.internal_data_buffer.clear();

                } else if byte_count == 0 || byte_count == -2 {

                    let _ = data_out.send(FullDuplexTcpState::UpStreamShutDown);
                    let _ = tls_stream.shutdown();
                    return;
                } else if byte_count == -1 {
                    continue;
                }
            } else {
            }
        }
    }

    fn get_data_stream<S: Read>(stream: &mut S, internal_data_buffer: &mut Vec<u8>) -> Option<i64> {

        let mut data_length: i64 = 0;

        loop {

            let mut r_buf = [0; 1024];

            match stream.read(&mut r_buf) {

                Ok(bytes_read) => {

                    if bytes_read == 0 {

                        break;

                    } else if bytes_read != 0 && bytes_read <= 1024 {

                        /*
                        let mut tmp_buf = r_buf.to_vec();
                        tmp_buf.truncate(bytes_read);
                        */
                        

                        //let _bw = self.internal_data_buffer.write(&tmp_buf).unwrap();

                        let _bw = internal_data_buffer.write(r_buf.split_at(bytes_read).0).unwrap();
                        data_length += bytes_read as i64;

                    } else {
                        println!("[+] Else hit!!!!!!!!!!!!!!!!!!!!!!");
                    }
                },
                Err(e) => {
                    match e.kind() {

                        io::ErrorKind::WouldBlock => {
                            if data_length == 0 {
                                data_length = -1;
                            }
                            break;
                        },
                        io::ErrorKind::ConnectionReset => {
                            data_length = -2;
                            break;
                        },
                        _ => {println!("[!!!] Got error: {}",e);}
                    }
                },
            }
        }
        return Some(data_length);
    }
}