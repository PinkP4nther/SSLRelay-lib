use std::time::Duration;
use openssl::ssl::{SslConnector, SslMethod, SslStream, SslVerifyMode};
use std::io::{self, Read, Write};
use std::net::{TcpStream, Shutdown};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::result::Result;
use std::sync::{Arc, Mutex};

use crate::{HandlerCallbacks, CallbackRet, InnerHandlers, TCPDataType};

#[derive(Debug)]
enum FullDuplexTcpState {
    DownStreamWrite(Vec<u8>),
    UpStreamWrite(Vec<u8>),
    DownStreamShutDown,
    UpStreamShutDown,
}

#[derive(Debug)]
enum DataPipe {
    DataWrite(Vec<u8>),
    Shutdown,
}

/*
trait DataStream {}
impl<S: Read + Write> DataStream for S {} 
*/
pub enum DataStreamType {
    RAW(TcpStream),
    TLS(SslStream<TcpStream>),
}

//impl Read for DataStreamType {}
//impl Write for DataStreamType {}
/*
impl DataStreamType {
    fn shutdown(&self) {
        match self {
            &Self::RAW(r) => {
                r.shutdown(Shutdown::Both);
                return;
            },
            &Self::TLS(t) => {
                t.shutdown();
                return;
            },
        }
    }
}
*/
struct DownStreamInner
{
    //ds_stream: Option<Arc<Mutex<SslStream<TcpStream>>>>,
    //ds_stream: Arc<Mutex<DataStreamType>>,
    ds_stream: DataStreamType,
    internal_data_buffer: Vec<u8>,
}

impl DownStreamInner {

    fn handle_error(error_description: &str) {
        println!("[SSLRelay DownStream Thread Error]: {}", error_description);
        //self.ds_stream.as_ref().shutdown();
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
/*
                            let mut stream_lock = match self.ds_stream.as_ref().unwrap().lock() {
                                Ok(sl) => sl,
                                Err(_e) => {
                                    self.handle_error("Failed to get stream lock!");
                                    if let Err(e) = data_out.send(FullDuplexTcpState::DownStreamShutDown) {
                                        self.handle_error(format!("Failed to send shutdown signal to main thread from DownStream thread: {}", e).as_str());
                                    }
                                    return;
                                }
                            };
*/
                            match raw_stream.write_all(&data) {
                                Ok(()) => {},
                                Err(_e) => {
                                    Self::handle_error("Failed to write data to DownStream tcp stream!");
                                    if let Err(e) = data_out.send(FullDuplexTcpState::DownStreamShutDown) {
                                        Self::handle_error(format!("Failed to send shutdown signal to main thread from DownStream thread: {}", e).as_str());
                                        let _ = raw_stream.shutdown(Shutdown::Both);
                                    }
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
                        Self::handle_error(format!("Failed to send UpStreamWrite to main thread: {}", e).as_str());
                        return;
                    }

                    self.internal_data_buffer.clear();

                } else if byte_count == 0 || byte_count == -2 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::DownStreamShutDown) {
                        Self::handle_error(format!("Failed to send shutdown signal to main thread from DownStream thread: {}", e).as_str());
                    }
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
/*
                            let mut stream_lock = match self.ds_stream.as_ref().unwrap().lock() {
                                Ok(sl) => sl,
                                Err(_e) => {
                                    self.handle_error("Failed to get stream lock!");
                                    if let Err(e) = data_out.send(FullDuplexTcpState::DownStreamShutDown) {
                                        self.handle_error(format!("Failed to send shutdown signal to main thread from DownStream thread: {}", e).as_str());
                                    }
                                    return;
                                }
                            };
*/
                            match tls_stream.write_all(&data) {
                                Ok(()) => {},
                                Err(_e) => {
                                    Self::handle_error("Failed to write data to DownStream tcp stream!");
                                    if let Err(e) = data_out.send(FullDuplexTcpState::DownStreamShutDown) {
                                        Self::handle_error(format!("Failed to send shutdown signal to main thread from DownStream thread: {}", e).as_str());
                                    }
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
                        Self::handle_error(format!("Failed to send UpStreamWrite to main thread: {}", e).as_str());
                        return;
                    }

                    self.internal_data_buffer.clear();

                } else if byte_count == 0 || byte_count == -2 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::DownStreamShutDown) {
                        Self::handle_error(format!("Failed to send shutdown signal to main thread from DownStream thread: {}", e).as_str());
                    }
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

        //let mut stream_lock = self.ds_stream.as_mut().unwrap().lock().unwrap();

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

struct UpStreamInner
{
    //us_stream: Option<Arc<Mutex<DataStreamType>>>,
    us_stream: DataStreamType,
    internal_data_buffer: Vec<u8>
}

impl UpStreamInner {

    fn handle_error(error_description: &str) {
        println!("[SSLRelay UpStream Thread Error]: {}", error_description);
        //let _ = self.us_stream.as_ref().unwrap().lock().unwrap().get_ref().shutdown(Shutdown::Both);
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
/*
                            let mut stream_lock = match self.us_stream.as_ref().unwrap().lock() {
                                Ok(sl) => sl,
                                Err(_e) => {
                                    self.handle_error("Failed to get stream lock!");
                                    if let Err(_e) = data_out.send(FullDuplexTcpState::UpStreamShutDown) {
                                        //println!("[SSLRelay UpStream Thread Error]: Failed to send shutdown signal to main thread from UpStream thread: {}", e);
                                        //return;
                                    }
                                    return;
                                }
                            };
*/
                            match raw_stream.write_all(&data) {
                                Ok(()) => {},
                                Err(_e) => {
                                    println!("Failed to write data to UpStream tcp stream!");
                                    if let Err(_e) = data_out.send(FullDuplexTcpState::UpStreamShutDown) {
                                        //println!("[SSLRelay UpStream Thread Error]: Failed to send shutdown signal to main thread from UpStream thread: {}", e);
                                        //return;
                                    }
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
                        Self::handle_error(format!("Failed to send DownStreamWrite to main thread: {}", e).as_str());
                        return;
                    }

                    self.internal_data_buffer.clear();

                } else if byte_count == 0 || byte_count == -2 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::UpStreamShutDown) {
                        Self::handle_error(format!("Failed to send shutdown signal to main thread from UpStream thread: {}", e).as_str());
                    }
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
/*
                            let mut stream_lock = match self.us_stream.as_ref().unwrap().lock() {
                                Ok(sl) => sl,
                                Err(_e) => {
                                    self.handle_error("Failed to get stream lock!");
                                    if let Err(_e) = data_out.send(FullDuplexTcpState::UpStreamShutDown) {
                                        //println!("[SSLRelay UpStream Thread Error]: Failed to send shutdown signal to main thread from UpStream thread: {}", e);
                                        //return;
                                    }
                                    return;
                                }
                            };
*/
                            match tls_stream.write_all(&data) {
                                Ok(()) => {},
                                Err(_e) => {
                                    println!("Failed to write data to UpStream tcp stream!");
                                    if let Err(_e) = data_out.send(FullDuplexTcpState::UpStreamShutDown) {
                                        //println!("[SSLRelay UpStream Thread Error]: Failed to send shutdown signal to main thread from UpStream thread: {}", e);
                                        //return;
                                    }
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
                        Self::handle_error(format!("Failed to send DownStreamWrite to main thread: {}", e).as_str());
                        return;
                    }

                    self.internal_data_buffer.clear();

                } else if byte_count == 0 || byte_count == -2 {

                    if let Err(e) = data_out.send(FullDuplexTcpState::UpStreamShutDown) {
                        Self::handle_error(format!("Failed to send shutdown signal to main thread from UpStream thread: {}", e).as_str());
                    }
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

        //let mut stream_lock = self.us_stream.as_mut().unwrap().lock().unwrap();

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

#[allow(dead_code)]
pub struct FullDuplexTcp<H>
where
    H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static,
{
    //ds_tcp_stream: Arc<Mutex<DataStreamType>>,
    //us_tcp_stream: Option<Arc<Mutex<DataStreamType>>>,
    //remote_endpoint: String,
    remote_host: String,
    remote_port: String,
    ds_inner_m: Arc<Mutex<Option<DownStreamInner>>>,
    //ds_inner_m: DownStreamInner,
    us_inner_m: Arc<Mutex<Option<UpStreamInner>>>,
    //us_inner_m: UpStreamInner,
    inner_handlers: InnerHandlers<H>,
}

impl<H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static> FullDuplexTcp<H> {

    pub fn new(ds_tcp_stream: DataStreamType, us_tcp_stream_type: TCPDataType, remote_host: String, remote_port: String, handlers: InnerHandlers<H>) -> Result<Self, i8> {

        //let _ = ds_tcp_stream.set_read_timeout(Some(Duration::from_millis(50)));
        match ds_tcp_stream {
            DataStreamType::RAW(ref s) => { let _ = s.set_read_timeout(Some(Duration::from_millis(50))); },
            DataStreamType::TLS(ref s) => { let _ = s.get_ref().set_read_timeout(Some(Duration::from_millis(50))); },
        }

        // Connect to remote here
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
            //ds_tcp_stream: Arc::new(Mutex::new(ds_tcp_stream)),
            //us_tcp_stream: None,
            remote_host,
            remote_port,
            ds_inner_m: Arc::new(Mutex::new(Some(DownStreamInner{ds_stream: ds_tcp_stream, internal_data_buffer: Vec::<u8>::new()}))),
            //ds_inner_m: DownStreamInner{ds_stream: ds_tcp_stream, internal_data_buffer: Vec::<u8>::new()},
            us_inner_m: Arc::new(Mutex::new(Some(UpStreamInner{us_stream: us_tcp_stream, internal_data_buffer: Vec::<u8>::new()}))),
            //us_inner_m: UpStreamInner{us_stream: us_tcp_stream, internal_data_buffer: Vec::<u8>::new()},
            inner_handlers: handlers,
        })
    }

    pub fn handle(&mut self) {
/*
        if self.connect_endpoint() < 0 {
            let _ = self.ds_tcp_stream.lock().unwrap().get_ref().shutdown(Shutdown::Both);
            return;
        }
*/
        let (state_sender, state_receiver): (Sender<FullDuplexTcpState>, Receiver<FullDuplexTcpState>) = mpsc::channel();
        let (ds_data_pipe_sender, ds_data_pipe_receiver): (Sender<DataPipe>, Receiver<DataPipe>) = mpsc::channel();
        let (us_data_pipe_sender, us_data_pipe_receiver): (Sender<DataPipe>, Receiver<DataPipe>) = mpsc::channel();

        //self.ds_inner_m.lock().unwrap().ds_stream = Some(self.ds_tcp_stream.clone());
        let ds_method_pointer = self.ds_inner_m.clone();
        let ds_state_bc = state_sender.clone();

        //self.us_inner_m.lock().unwrap().us_stream = Some(self.us_tcp_stream.as_ref().unwrap().clone());
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
        /*
                self.us_tcp_stream = Some(
                    Arc::new(
                        Mutex::new(
                            s
                )));*/
                let _ = s.get_ref().set_read_timeout(Some(Duration::from_millis(50)));
                return Ok(DataStreamType::TLS(s));
            }
        }
    }

    fn handle_error(error_description: &str) {
        println!("[SSLRelay Master Thread Error]: {}", error_description);
    }
}