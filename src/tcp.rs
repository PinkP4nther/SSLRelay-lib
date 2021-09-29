use crate::{
    FullDuplexTcp,
    HandlerCallbacks,
    DataStreamType,
    TCPDataType,
    Duration,
    Arc,
    Mutex,
    DownStreamInner,
    UpStreamInner,
    InnerHandlers,
    Shutdown,
    Sender,
    Receiver,
    FullDuplexTcpState,
    DataPipe,
    mpsc,
    thread,
    CallbackRet,
    TcpStream,
    SslVerifyMode,
    SslConnector,
    SslMethod,
};

impl<H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static> FullDuplexTcp<H> {

    pub fn new(ds_tcp_stream: DataStreamType, us_tcp_stream_type: TCPDataType, remote_host: String, remote_port: String, handlers: InnerHandlers<H>) -> Result<Self, i8> {

        match ds_tcp_stream {
            DataStreamType::RAW(ref s) => { let _ = s.set_read_timeout(Some(Duration::from_millis(50))); },
            DataStreamType::TLS(ref s) => { let _ = s.get_ref().set_read_timeout(Some(Duration::from_millis(50))); },
        }

        let us_tcp_stream = match Self::connect_endpoint(us_tcp_stream_type, remote_host.clone(), remote_port.clone()) {
            Ok(s) => s,
            Err(ec) => {
                match ds_tcp_stream {
                    DataStreamType::RAW(s) => { let _ = s.shutdown(Shutdown::Both); },
                    DataStreamType::TLS(mut s) => { let _ = s.shutdown(); },
                }
                return Err(ec);
            }
        };

        Ok(
            FullDuplexTcp {
            remote_host,
            remote_port,
            ds_inner_m: Arc::new(Mutex::new(Some(DownStreamInner{ds_stream: ds_tcp_stream, internal_data_buffer: Vec::<u8>::new()}))),
            us_inner_m: Arc::new(Mutex::new(Some(UpStreamInner{us_stream: us_tcp_stream, internal_data_buffer: Vec::<u8>::new()}))),
            inner_handlers: handlers,
        })
    }

    pub fn handle(&mut self) {

        let (state_sender, state_receiver): (Sender<FullDuplexTcpState>, Receiver<FullDuplexTcpState>) = mpsc::channel();
        let (ds_data_pipe_sender, ds_data_pipe_receiver): (Sender<DataPipe>, Receiver<DataPipe>) = mpsc::channel();
        let (us_data_pipe_sender, us_data_pipe_receiver): (Sender<DataPipe>, Receiver<DataPipe>) = mpsc::channel();

        let ds_method_pointer = self.ds_inner_m.clone();
        let ds_state_bc = state_sender.clone();

        let us_method_pointer = self.us_inner_m.clone();
        let us_state_bc = state_sender.clone();

        thread::spawn(move || {
            ds_method_pointer.lock().unwrap().take().unwrap().ds_handler(ds_state_bc, ds_data_pipe_receiver);
        });

        thread::spawn(move || {
            us_method_pointer.lock().unwrap().take().unwrap().us_handler(us_state_bc, us_data_pipe_receiver);
        });

        loop {

            match state_receiver.recv() {
                Ok(state_request) => {
                    match state_request {

                        // DownStream Write Request
                        FullDuplexTcpState::DownStreamWrite(data) => {

                            /*
                                Callbacks that work with data from UpStream go here
                                Add callback return types for blocking callback subroutines
                                Shutdown - Shutdown TCP connection
                                Relay - Relay TCP stream
                                Spoof - Spoof back to received stream direction
                                Freeze - Freeze data (dont relay and destroy data)
                            */

                            let inner_handlers_clone = self.inner_handlers.clone();
                            let in_data = data.clone();

                            thread::spawn(move || {
                                inner_handlers_clone.cb.us_nb_callback(in_data);
                            });

                            match self.inner_handlers.cb.us_b_callback(data) {
                                CallbackRet::Relay(retdata) => {
                                    match ds_data_pipe_sender.send(DataPipe::DataWrite(retdata)) {
                                        Ok(()) => {},
                                        Err(e) => {
                                            Self::handle_error(format!("Failed to send data write to DownStream thread: {}", e).as_str());
                                            return;
                                        }
                                    }
                                },
                                CallbackRet::Spoof(retdata) => {
                                    match us_data_pipe_sender.send(DataPipe::DataWrite(retdata)) {
                                        Ok(()) => {},
                                        Err(e) => {
                                            Self::handle_error(format!("Failed to send data write to DownStream thread: {}", e).as_str());
                                            return;
                                        }
                                    }
                                },
                                CallbackRet::Freeze => {},
                                CallbackRet::Shutdown => {
                                    if let Err(e) = us_data_pipe_sender.send(DataPipe::Shutdown) {
                                        Self::handle_error(format!("Failed to send Shutdown signal to UpStream thread: {}", e).as_str());
                                    }
                                    if let Err(e) = ds_data_pipe_sender.send(DataPipe::Shutdown) {
                                        Self::handle_error(format!("Failed to send Shutdown signal to DownStream thread: {}", e).as_str());
                                    }
                                    return;
                                }
                            }

                        },
                        // UpStream Write Request
                        FullDuplexTcpState::UpStreamWrite(data) => {

                            /*
                                Callbacks that work with data from DownStream go here
                            */

                            let inner_handlers_clone = self.inner_handlers.clone();
                            let in_data = data.clone();

                            thread::spawn(move || {
                                inner_handlers_clone.cb.ds_nb_callback(in_data);
                            });

                            match self.inner_handlers.cb.ds_b_callback(data) {
                                CallbackRet::Relay(retdata) => {
                                    match us_data_pipe_sender.send(DataPipe::DataWrite(retdata)) {
                                        Ok(()) => {},
                                        Err(e) => {
                                            Self::handle_error(format!("Failed to send data write to UpStream thread: {}", e).as_str());
                                            return;
                                        }
                                    }
                                },
                                CallbackRet::Spoof(retdata) => {
                                    match ds_data_pipe_sender.send(DataPipe::DataWrite(retdata)) {
                                        Ok(()) => {},
                                        Err(e) => {
                                            Self::handle_error(format!("Failed to send data write to DownStream thread: {}", e).as_str());
                                            return;
                                        }
                                    }
                                },
                                CallbackRet::Freeze => {},
                                CallbackRet::Shutdown => {
                                    if let Err(e) = ds_data_pipe_sender.send(DataPipe::Shutdown) {
                                        Self::handle_error(format!("Failed to send Shutdown signal to DownStream thread: {}", e).as_str());
                                    }
                                    if let Err(e) = us_data_pipe_sender.send(DataPipe::Shutdown) {
                                        Self::handle_error(format!("Failed to send Shutdown signal to UpStream thread: {}", e).as_str());
                                    }
                                    return;
                                }
                            }
                        },
                        // DownStreamShutDown Request
                        FullDuplexTcpState::DownStreamShutDown => {

                            if let Err(e) = us_data_pipe_sender.send(DataPipe::Shutdown) {
                                Self::handle_error(format!("Failed to send Shutdown signal to UpStream thread: {}", e).as_str());
                                return;
                            }
                            return;
                        },
                        // UpStreamShutDown Request
                        FullDuplexTcpState::UpStreamShutDown => {

                            if let Err(e) = ds_data_pipe_sender.send(DataPipe::Shutdown) {
                                Self::handle_error(format!("Failed to send Shutdown signal to DownStream thread: {}", e).as_str());
                                return;
                            }
                            return;
                        },
                    }
                },
                Err(_e) => {
                    Self::handle_error("State receiver communication channel has closed!");
                    if let Err(e) = ds_data_pipe_sender.send(DataPipe::Shutdown) {
                        Self::handle_error(format!("Failed to send Shutdown signal to DownStream thread: {}", e).as_str());
                    }
                    if let Err(e) = us_data_pipe_sender.send(DataPipe::Shutdown) {
                        Self::handle_error(format!("Failed to send Shutdown signal to UpStream thread: {}", e).as_str());
                    }
                    return;
                }
            }// State Receiver
        }
    }
    
    fn connect_endpoint(stream_data_type: TCPDataType, remote_host: String, remote_port: String) -> Result<DataStreamType, i8> {

        match stream_data_type {

            TCPDataType::RAW => {
                let s = match TcpStream::connect(format!("{}:{}", remote_host, remote_port)) {
                    Ok(s) => s,
                    Err(e) => {
                        Self::handle_error(format!("Can't connect to remote host: {}\nErr: {}", format!("{}:{}", remote_host, remote_port), e).as_str());
                        return Result::Err(-1);
                    }
                };
                let _ = s.set_read_timeout(Some(Duration::from_millis(50)));
                return Ok(DataStreamType::RAW(s));

            },
            TCPDataType::TLS => {

                let mut sslbuilder = SslConnector::builder(SslMethod::tls()).unwrap();
                sslbuilder.set_verify(SslVerifyMode::NONE);
        
                let connector = sslbuilder.build();
        
                let s = match TcpStream::connect(format!("{}:{}", remote_host, remote_port)) {
                    Ok(s) => s,
                    Err(e) => {
                        Self::handle_error(format!("Can't connect to remote host: {}\nErr: {}", format!("{}:{}", remote_host, remote_port), e).as_str());
                        return Result::Err(-1);
                    }
                };
        
                let s = match connector.connect(remote_host.as_str(), s) {
                    Ok(s) => s,
                    Err(e) => {
                        Self::handle_error(format!("Failed to accept TLS/SSL handshake: {}", e).as_str());
                        return Result::Err(-2);
                    }
                };

                let _ = s.get_ref().set_read_timeout(Some(Duration::from_millis(50)));
                return Ok(DataStreamType::TLS(s));
            }
        }
    }

    fn handle_error(error_description: &str) {
        println!("[SSLRelay Master Thread Error]: {}", error_description);
    }
}