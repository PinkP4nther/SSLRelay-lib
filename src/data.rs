use std::time::Duration;
use openssl::ssl::{SslConnector, SslMethod, SslStream, SslVerifyMode};
use std::io::{self, Read, Write};
use std::net::{TcpStream, Shutdown};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::sync::{Arc, Mutex};

use crate::{HandlerCallbacks, InnerHandlers};

#[derive(Debug)]
enum FullDuplexTcpState {
    DownStreamRead,
    DownStreamWrite(Vec<u8>),
    UpStreamRead,
    UpStreamWrite(Vec<u8>),
    DownStreamShutDown,
    UpStreamShutDown,
}

#[derive(Debug)]
enum DataPipe {
    DataWrite(Vec<u8>),
    Finished,
    Shutdown,
}

struct DownStreamInner {
    ds_stream: Option<Arc<Mutex<SslStream<TcpStream>>>>,
    internal_data_buffer: Vec<u8>,
}

impl DownStreamInner {
    pub fn ds_handler(&mut self, data_out: Sender<FullDuplexTcpState>, data_in: Receiver<DataPipe>) {

        loop {

            match data_in.recv_timeout(Duration::from_millis(50)) {

                // DataPipe Received
                Ok(data_received) => {

                    match data_received {
                        DataPipe::DataWrite(data) => {

                            let mut stream_lock = match self.ds_stream.as_ref().unwrap().lock() {
                                Ok(sl) => sl,
                                Err(_e) => {
                                    println!("[!] Failed to get stream lock!");
                                    return;
                                }
                            };

                            match stream_lock.write_all(&data) {
                                Ok(()) => {},
                                Err(_e) => {
                                    println!("[!] Failed to write data to DownStream tcp stream!");
                                }
                            }
                            let _ = stream_lock.flush();
                            drop(stream_lock);

                            if let Err(e) = data_out.send(FullDuplexTcpState::DownStreamRead) {
                                println!("[SSLRelay DownStream Thread Error]: Failed to send DownStreamRead ready notifier to main thread: {}", e);
                                return;
                            }
                        },
                        DataPipe::Shutdown => {
                            let _ = self.ds_stream.as_ref().unwrap().lock().unwrap().get_ref().shutdown(Shutdown::Both);
                            return;
                        },
                        _ => {}
                    }
                },
                Err(_e) => {
                    match _e {
                        mpsc::RecvTimeoutError::Timeout => {},
                        mpsc::RecvTimeoutError::Disconnected => {
                            println!("[!] DownStream data_in channel is disconnected!");
                            return;
                        }
                    }
                }
            }// End of data_in receive

            // If received data
            if let Some(byte_count) = self.get_data_stream() {
                if byte_count > 0 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::UpStreamWrite(self.internal_data_buffer.clone())) {
                        println!("[SSLRelay DownStream Thread Error]: Failed to send UpStreamWrite to main thread: {}", e);
                        return;
                    }

                    // Possible race condition here if both DownStream and UpStream recieve data at same time!!!

                    match data_in.recv() {
                        Ok(data) => {
                            match data {
                                DataPipe::Finished => {
                                    self.internal_data_buffer.clear();
                                    continue;
                                },
                                _ => {
                                    println!("[SSLRelay DownStream Thread Error]: Got unexpected data from data_in: Expected [Finished] but got {:?}", data);
                                    return;
                                }
                            }
                        },
                        Err(e) => {
                            println!("[SSLRelay DownStream Thread Error]: Failed to receive data from data_in: {}", e);
                            return;
                        }
                    }

                } else if byte_count == 0 || byte_count == -2 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::DownStreamShutDown) {
                        println!("[SSLRelay DownStream Thread Error]: Failed to send shutdown signal to main thread from DownStream thread: {}", e);
                        return;
                    }
                    let _ = self.ds_stream.as_ref().unwrap().lock().unwrap().get_ref().shutdown(Shutdown::Both);
                    return;
                } else if byte_count == -1 {
                    continue;
                }
            } else {

            }
        }
    }

    fn get_data_stream(&mut self) -> Option<i64> {

        let mut data_length: i64 = 0;

        let mut stream_lock = self.ds_stream.as_mut().unwrap().lock().unwrap();

        loop {

            let mut r_buf = [0; 1024];

            match stream_lock.read(&mut r_buf) {

                Ok(bytes_read) => {

                    if bytes_read == 0 {
                        break;

                    } else if bytes_read != 0 && bytes_read <= 1024 {

                        /*
                        let mut tmp_buf = r_buf.to_vec();
                        tmp_buf.truncate(bytes_read);
                        */
                        

                        //let _bw = self.internal_data_buffer.write(&tmp_buf).unwrap();

                        let _bw = self.internal_data_buffer.write(r_buf.split_at(bytes_read).0).unwrap();
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

struct UpStreamInner{
    us_stream: Option<Arc<Mutex<SslStream<TcpStream>>>>,
    internal_data_buffer: Vec<u8>
}

impl UpStreamInner {
    pub fn us_handler(&mut self, data_out: Sender<FullDuplexTcpState>, data_in: Receiver<DataPipe>) {

        loop {

            match data_in.recv_timeout(Duration::from_millis(50)) {

                Ok(data_received) => {

                    match data_received {
                        DataPipe::DataWrite(data) => {

                            let mut stream_lock = match self.us_stream.as_ref().unwrap().lock() {
                                Ok(sl) => sl,
                                Err(_e) => {
                                    println!("[!] Failed to get stream lock!");
                                    return;
                                }
                            };

                            match stream_lock.write_all(&data) {
                                Ok(()) => {},
                                Err(_e) => {
                                    println!("[!] Failed to write data to DownStream tcp stream!");
                                }
                            }
                            let _ = stream_lock.flush();
                            drop(stream_lock);

                            if let Err(e) = data_out.send(FullDuplexTcpState::UpStreamRead) {
                                println!("[SSLRelay UpStream Thread Error]: Failed to send UpStreamRead ready notifier to main thread: {}", e);
                                return;
                            }
                        },
                        DataPipe::Shutdown => {
                            self.us_stream.as_ref().unwrap().lock().unwrap().get_ref().shutdown(Shutdown::Both).unwrap();
                            return;
                        }
                        _ => {}
                    }
                },
                Err(e) => {
                    match e {
                        mpsc::RecvTimeoutError::Timeout => {},
                        mpsc::RecvTimeoutError::Disconnected => {
                            println!("[!] UpStream data_in channel is disconnected!");
                            return;
                        }
                    }
                }
            }// End of data_in receive

            if let Some(byte_count) = self.get_data_stream() {
                if byte_count > 0 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::DownStreamWrite(self.internal_data_buffer.clone())) {
                        println!("[SSLRelay UpStream Thread Error]: Failed to send DownStreamWrite to main thread: {}", e);
                        return;
                    }

                    match data_in.recv() {
                        Ok(data) => {
                            match data {
                                DataPipe::Finished => {
                                    self.internal_data_buffer.clear();
                                    continue;
                                },
                                _ => {
                                    println!("[SSLRelay UpStream Thread Error]: Got unexpected data from data_in: Expected [Finished] but got {:?}", data);
                                    return;
                                }
                            }
                        },
                        Err(e) => {
                            println!("[SSLRelay UpStream Thread Error]: Failed to receive data from data_in: {}", e);
                            return;
                        }
                    }

                } else if byte_count == 0 || byte_count == -2 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::UpStreamShutDown) {
                        println!("[SSLRelay UpStream Thread Error]: Failed to send shutdown signal to main thread from UpStream thread: {}", e);
                        return;
                    }
                    let _ = self.us_stream.as_ref().unwrap().lock().unwrap().get_ref().shutdown(Shutdown::Both);
                    return;
                } else if byte_count == -1 {
                    continue;
                }
            } else {
            }
        }
    }

    fn get_data_stream(&mut self) -> Option<i64> {

        let mut data_length: i64 = 0;

        let mut stream_lock = self.us_stream.as_mut().unwrap().lock().unwrap();

        loop {

            let mut r_buf = [0; 1024];

            match stream_lock.read(&mut r_buf) {

                Ok(bytes_read) => {

                    if bytes_read == 0 {

                        break;

                    } else if bytes_read != 0 && bytes_read <= 1024 {

                        /*
                        let mut tmp_buf = r_buf.to_vec();
                        tmp_buf.truncate(bytes_read);
                        */
                        

                        //let _bw = self.internal_data_buffer.write(&tmp_buf).unwrap();

                        let _bw = self.internal_data_buffer.write(r_buf.split_at(bytes_read).0).unwrap();
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

pub struct FullDuplexTcp<H>
where
    H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static,
{
    ds_tcp_stream: Arc<Mutex<SslStream<TcpStream>>>,
    us_tcp_stream: Option<Arc<Mutex<SslStream<TcpStream>>>>,
    remote_endpoint: String,
    ds_inner_m: Arc<Mutex<DownStreamInner>>,
    us_inner_m: Arc<Mutex<UpStreamInner>>,
    inner_handlers: InnerHandlers<H>,
}

impl<H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static> FullDuplexTcp<H> {

    pub fn new(ds_tcp_stream: SslStream<TcpStream>, remote_endpoint: String, handlers: InnerHandlers<H>) -> Self {

        let _ = ds_tcp_stream.get_ref().set_read_timeout(Some(Duration::from_millis(50)));

        FullDuplexTcp {
            ds_tcp_stream: Arc::new(Mutex::new(ds_tcp_stream)),
            us_tcp_stream: None,
            remote_endpoint,
            ds_inner_m: Arc::new(Mutex::new(DownStreamInner{ds_stream: None, internal_data_buffer: Vec::<u8>::new()})),
            us_inner_m: Arc::new(Mutex::new(UpStreamInner{us_stream: None, internal_data_buffer: Vec::<u8>::new()})),
            inner_handlers: handlers,
        }
    }

    pub fn handle(&mut self) {

        if self.connect_endpoint() == -1 {
            let _ = self.ds_tcp_stream.lock().unwrap().get_ref().shutdown(Shutdown::Both);
            return;
        }

        let (state_sender, state_receiver): (Sender<FullDuplexTcpState>, Receiver<FullDuplexTcpState>) = mpsc::channel();
        let (ds_data_pipe_sender, ds_data_pipe_receiver): (Sender<DataPipe>, Receiver<DataPipe>) = mpsc::channel();
        let (us_data_pipe_sender, us_data_pipe_receiver): (Sender<DataPipe>, Receiver<DataPipe>) = mpsc::channel();

        self.ds_inner_m.lock().unwrap().ds_stream = Some(self.ds_tcp_stream.clone());
        let ds_method_pointer = self.ds_inner_m.clone();
        let ds_state_bc = state_sender.clone();

        self.us_inner_m.lock().unwrap().us_stream = Some(self.us_tcp_stream.as_ref().unwrap().clone());
        let us_method_pointer = self.us_inner_m.clone();
        let us_state_bc = state_sender.clone();

        thread::spawn(move || {
            ds_method_pointer.lock().unwrap().ds_handler(ds_state_bc, ds_data_pipe_receiver);
        });

        thread::spawn(move || {
            us_method_pointer.lock().unwrap().us_handler(us_state_bc, us_data_pipe_receiver);
        });

        loop {

            match state_receiver.recv() {
                Ok(state_request) => {
                    match state_request {

                        // DownStream Write Request
                        FullDuplexTcpState::DownStreamWrite(mut data) => {

                            /*
                                Callbacks that work with data from UpStream go here
                            */

                            let inner_handlers_clone = self.inner_handlers.clone();
                            let in_data = data.clone();

                            thread::spawn(move || {
                                inner_handlers_clone.cb.us_nb_callback(in_data);
                            });

                            self.inner_handlers.cb.us_b_callback(&mut data);
                            match ds_data_pipe_sender.send(DataPipe::DataWrite(data)) {
                                Ok(()) => {},
                                Err(e) => {
                                    println!("[SSLRelay Error]: Failed to send data write to DownStream thread: {}", e);
                                    return;
                                }
                            }

                            match state_receiver.recv() {
                                Ok(data) => {
                                    match data {
                                        FullDuplexTcpState::DownStreamRead => {
                                            if let Err(e) = us_data_pipe_sender.send(DataPipe::Finished) {
                                                println!("[SSLRelay Error]: Failed to send Finished notifier to UpStream thread: {}", e);
                                                return;
                                            }
                                        },
                                        _ => {
                                            println!("[SSLRelay Error]: Got unexpected data from state_receiver: Expected [DownStreamRead] but got {:?}", data);
                                            return;
                                        }
                                    }
                                },
                                Err(e) => {
                                    println!("[SSLRelay Error]: Failed to receive DownStream read from state_receiver: {}", e);
                                    return;
                                }
                            }
                        },
                        // UpStream Write Request
                        FullDuplexTcpState::UpStreamWrite(mut data) => {

                            /*
                                Callbacks that work with data from DownStream go here
                            */

                            let inner_handlers_clone = self.inner_handlers.clone();
                            let in_data = data.clone();

                            thread::spawn(move || {
                                inner_handlers_clone.cb.ds_nb_callback(in_data);
                            });

                            self.inner_handlers.cb.ds_b_callback(&mut data);

                            match us_data_pipe_sender.send(DataPipe::DataWrite(data)) {
                                Ok(()) => {},
                                Err(e) => {
                                    println!("[SSLRelay Error]: Failed to send data write to UpStream thread: {}", e);
                                    return;
                                }
                            }

                            match state_receiver.recv() {
                                Ok(data) => {
                                    match data {
                                        FullDuplexTcpState::UpStreamRead => {
                                            if let Err(e) = ds_data_pipe_sender.send(DataPipe::Finished) {
                                                println!("[SSLRelay Error]: Failed to send Finished notifier to DownStream thread: {}", e);
                                                return;
                                            }
                                        },
                                        _ => {
                                            println!("[SSLRelay Error]: Got unexpected data from state_receiver: Expected [UpStreamRead] but got {:?}", data);
                                            return;
                                        }
                                    }
                                },
                                Err(e) => {
                                    println!("[SSLRelay Error]: Failed to receive UpStream read from state_receiver: {}", e);
                                    return;
                                }
                            }
                        },
                        // DownStreamShutDown Request
                        FullDuplexTcpState::DownStreamShutDown => {

                            if let Err(e) = us_data_pipe_sender.send(DataPipe::Shutdown) {
                                println!("[SSLRelay Error]: Failed to send Shutdown signal to UpStream thread: {}", e);
                                return;
                            }
                            return;
                        },
                        // UpStreamShutDown Request
                        FullDuplexTcpState::UpStreamShutDown => {

                            if let Err(e) = ds_data_pipe_sender.send(DataPipe::Shutdown) {
                                println!("[SSLRelay Error]: Failed to send Shutdown signal to DownStream thread: {}", e);
                                return;
                            }
                            return;
                        },
                        _ => {}
                    }
                },
                Err(_e) => {
                    println!("[!] State receiver communication channel has closed!");
                    return;
                }
            }
        }
    }
    
    fn connect_endpoint(&mut self) -> i8 {

        let mut sslbuilder = SslConnector::builder(SslMethod::tls()).unwrap();
        sslbuilder.set_verify(SslVerifyMode::NONE);

        let connector = sslbuilder.build();

        let s = match TcpStream::connect(self.remote_endpoint.as_str()) {
            Ok(s) => s,
            Err(e) => {
                println!("[!] Can't connect to remote host: {}\nErr: {}", self.remote_endpoint, e);
                return -1;
            }
        };

        let r_host: Vec<&str> = self.remote_endpoint.as_str().split(":").collect();

        let s = connector.connect(r_host[0], s).unwrap();

        self.us_tcp_stream = Some(
            Arc::new(
            Mutex::new(
                s
            )));
        let _ = self.us_tcp_stream.as_ref().unwrap().lock().unwrap().get_ref().set_read_timeout(Some(Duration::from_millis(50)));
        return 0;
    }
}