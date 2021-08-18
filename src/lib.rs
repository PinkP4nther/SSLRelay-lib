use openssl::ssl::{SslAcceptor, SslConnector, SslFiletype, SslMethod, SslStream, SslVerifyMode};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::env;
use std::fs;

use toml::Value as TValue;

use chunked_transfer::Decoder;

use flate2::read::GzDecoder;

use httparse::{self, Header};

struct RelayedResponse<'a> {
    http_version: Option<u8>,
    http_code: Option<u16>,
    http_reason: Option<String>,
    http_headers: Option<Vec<Header<'a>>>,
    http_body: Option<String>,
}

struct RelayRequest<'a> {
    http_method: Option<String>,
    http_path: Option<String>,
    http_version: Option<u8>,
    http_headers: Option<Vec<Header<'a>>>,
    http_body: Option<String>,
}

#[derive(Clone)]
struct RelayConfig {
    bind_host: String,
    bind_port: String,
    remote_host: String,
    remote_port: String,
    ssl_private_key_path: String,
    ssl_cert_path: String,
    verbose_level: i8,

}

pub trait HandlerCallbacks {
    fn req_b_callback(&self, _in_data: &mut Vec<u8>){}
    fn req_nb_callback(&self, _in_data: Vec<u8>){}
    fn res_b_callback(&self, _in_data: &mut Vec<u8>){}
    fn res_nb_callback(&self, _in_data: Vec<u8>){}
}

pub enum ConfigType<T> {
    Env,
    Path(T),
    Default,
}

#[derive(Clone)]
pub struct SSLRelay<H>
where
    H: HandlerCallbacks + std::marker::Sync + std::marker::Send + 'static,
{
    
    config: Option<RelayConfig>,
    handlers: Option<H>,
}

impl<H: HandlerCallbacks + std::marker::Sync + std::marker::Send + 'static> SSLRelay<H> {

    pub fn new(handlers: H) -> Self {

        SSLRelay {
            config: None,
            handlers: Some(handlers),
        }
    }

    pub fn load_config(&mut self, config_path: ConfigType<String>) {
        self.config = Some(self.load_relay_config(config_path));
    }

    pub fn start(&mut self) {

        let rc_pointer = Arc::new(Mutex::new(self.config.as_ref().unwrap().clone()));
        let handler_pointer = Arc::new(Mutex::new(self.handlers.take().unwrap()));
        let acceptor = self.setup_ssl_config(self.config.as_ref().unwrap().ssl_private_key_path.clone(), self.config.as_ref().unwrap().ssl_cert_path.clone());
        let listener = TcpListener::bind(format!("{}:{}", self.config.as_ref().unwrap().bind_host.clone(), self.config.as_ref().unwrap().bind_port.clone())).unwrap();

        for stream in listener.incoming() {

            match stream {
                Ok(stream) => {

                    let acceptor = acceptor.clone();
                    let rc_config = rc_pointer.clone();
                    let handler = handler_pointer.clone();

                    thread::spawn(move || {

                        match acceptor.accept(stream) {
                            Ok(stream) => {

                                handle_stream(stream, rc_config, handler);
                                return 0;
                            },
                            Err(e) => {

                                println!("[Error] {}", e);
                                return -1;
                            }
                        }
                    });
                    /*let stream = acceptor.accept(stream).unwrap();
                    handle_stream(stream, rc_config, handler);*/
                },
                Err(e) => {println!("[Error] Tcp Connection Failed: {}", e)}
            }
        }
    }

    fn load_relay_config(&self, config_path: ConfigType<String>) -> RelayConfig {

        let mut resolved_path = String::from("./relay_config.toml");
        match config_path {
            ConfigType::Path(path) => {
                resolved_path = path.clone();
            },
            ConfigType::Env => {
                resolved_path = match env::var("SSLRELAY_CONFIG") {
                    Ok(p) => p.clone(),
                    Err(_e) => {
                        println!("[-] Environmental variable SSLRELAY_CONFIG does not exist.");
                        std::process::exit(-1);
                    }
                };
            },
            ConfigType::Default => {}
        }

        let bytes = fs::read(resolved_path).unwrap();
        let config_file = String::from_utf8_lossy(&bytes);
        let config_parsed = config_file.parse::<TValue>().unwrap();

        let bind_host = config_parsed["bind_host"].to_string().replace("\"", "");
        let bind_port = config_parsed["bind_port"].to_string().replace("\"", "");
        let ssl_private_key_path = config_parsed["ssl_private_key_path"].to_string().replace("\"", "");
        let ssl_cert_path = config_parsed["ssl_cert_path"].to_string().replace("\"", "");
        let remote_host = config_parsed["remote_host"].to_string().replace("\"", "");
        let remote_port = config_parsed["remote_port"].to_string().replace("\"", "");
        let verbose_level = config_parsed["verbose_level"].to_string().replace("\"", "").parse().unwrap();

        RelayConfig {
            bind_host: bind_host.clone(),
            bind_port: bind_port.clone(),
            ssl_private_key_path: ssl_private_key_path.clone(),
            ssl_cert_path: ssl_cert_path.clone(),
            remote_host: remote_host.clone(),
            remote_port: remote_port.clone(),
            verbose_level: verbose_level,
        }
    }

    fn setup_ssl_config(&self, priv_key: String, cert: String) -> Arc<SslAcceptor> {

        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_private_key_file(priv_key, SslFiletype::PEM).unwrap();
        acceptor.set_certificate_chain_file(cert).unwrap();
        acceptor.check_private_key().unwrap();
        Arc::new(acceptor.build())
    }




}// SSLRelay

#[derive(PartialEq)]
enum StreamDirection {
    Upstream,// Data coming from remote host
    DownStream,// Data coming from origin host
}

struct DataHandler {
    tcp_stream: Option<SslStream<TcpStream>>,
    relay_stream: Option<SslStream<TcpStream>>,
    remote_host: String,
    stream_direction: StreamDirection,
}

impl DataHandler {

    fn new(tcp_stream: SslStream<TcpStream>, remote_host: String) -> Self {
        let _ = tcp_stream.get_ref().set_read_timeout(Some(Duration::from_millis(100)));
        DataHandler {
            tcp_stream: Some(tcp_stream),
            relay_stream: None,
            remote_host,
            stream_direction: StreamDirection::DownStream,
        }
    }

    fn get_data_stream(&mut self, data: &mut Vec<u8>) -> usize {

        let mut data_length: usize = 0;
        if self.stream_direction == StreamDirection::DownStream {
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
        } else if self.stream_direction == StreamDirection::Upstream {
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
        return data_length;
    }

    fn http_req_verbose(&self, data: &Vec<u8>, mode: u8) {

        let req_info = self.get_request(&data);
        let req_header_list = req_info.http_headers.unwrap();
        let mut req_header_string = String::new();
    
        for header in req_header_list {
            if header.value.to_vec().len() == 0 {continue;}
            req_header_string.push_str(format!("[-->]   {}: {}\n", header.name, String::from_utf8(header.value.to_vec()).unwrap()).as_str());
        }
        if mode == 1 {
            println!("================================\n[-->] HTTP Version: {}\n[-->] HTTP Method: {} {}\n[-->] HTTP Headers:\n{}\n[-->] HTTP Body:\n{}\n"
                ,req_info.http_version.unwrap()
                ,req_info.http_method.unwrap()
                ,req_info.http_path.unwrap()
                ,req_header_string
                ,req_info.http_body.unwrap()
            );
        } else if mode == 2 {
            println!("[Req] {} {}", req_info.http_method.unwrap(), req_info.http_path.unwrap());
        }
    }
    
    fn http_res_verbose(&self, response_data: &Vec<u8>, mode: u8) {
    
        let res_info = self.get_response(&response_data);
        let header_list = res_info.http_headers.unwrap();
        let mut header_string = String::new();
    
        for header in header_list {
            if header.value.to_vec().len() == 0 {continue;}
            header_string.push_str(format!("[<--]   {}: {}\n", header.name, String::from_utf8(header.value.to_vec()).unwrap()).as_str());
        }
        if mode == 1 {
            println!("\n[<--] HTTP Version: {}\n[<--] HTTP Code: {} {}\n[<--] HTTP Headers:\n{}\n[<--] HTTP Body:\n{}\n================================\n"
                ,res_info.http_version.unwrap()
                ,res_info.http_code.unwrap()
                ,res_info.http_reason.unwrap()
                ,header_string
                ,res_info.http_body.unwrap()
            );
        } else if mode == 2 {
            println!("[Res] {} {}", res_info.http_code.unwrap(), res_info.http_reason.unwrap());
        }
    }

    fn relay_data(&mut self, data: &Vec<u8>) -> Option<i8> {

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
            //println!("[+] Relayed -> {}", host);
    
            //return Some(stream); Instead of returning we will now set stream object
            self.relay_stream = Some(stream);
            return Some(0);
        }
    }
    /*
    fn get_req_headers<'a>(&self, data: &'a Vec<u8>) -> Option<Vec<Header<'a>>> {
    
        let mut headers = [httparse::EMPTY_HEADER; 128];
        let mut request = httparse::Request::new(&mut headers);
    
        let req = request.parse(data).unwrap();
    
        if req.is_complete() {
            return Some(headers.to_vec());
        }
        None
    }
    
    fn get_res_headers(data: &Vec<u8>) -> Option<Vec<Header>> {
    
        let mut headers = [httparse::EMPTY_HEADER; 128];
        let mut response = httparse::Response::new(&mut headers);
    
        let res = response.parse(data).unwrap();
    
        if res.is_complete() {
            return Some(headers.to_vec());
        }
        None
    }
    
    fn get_host(&self, data: &Vec<u8>) -> Option<String> {
    
        return Some("137.220.37.67".to_string());
        let mut headers = [httparse::EMPTY_HEADER; 128];
    
        let mut request = httparse::Request::new(&mut headers);
    
        let req = request.parse(data).unwrap();
    
        if req.is_complete() {
            for header in headers.iter() {
                if header.name == "Host" || header.name == "host" {
                    //println!("Host -> {}", String::from_utf8(header.value.to_vec()).unwrap());
                    return Some(String::from_utf8(header.value.to_vec()).unwrap());
                }
            }
            None
        } else if req.is_partial() {
            for header in headers.iter() {
                if header.name == "Host" || header.name == "host" {
                    //println!("Host -> {}", String::from_utf8(header.value.to_vec()).unwrap());
                    return Some(String::from_utf8(header.value.to_vec()).unwrap());
                }
            }
            None
        } else {
            None
        }
    }
    
    fn get_cookie(&self, data: &Vec<u8>) -> Option<String> {
    
        let mut headers = [httparse::EMPTY_HEADER; 128];
    
        let mut request = httparse::Request::new(&mut headers);
    
        let req = request.parse(data).unwrap();
    
        if req.is_complete() {
            for header in headers.iter() {
                if header.name == "Cookie" || header.name == "cookie" {
                    //println!("Host -> {}", String::from_utf8(header.value.to_vec()).unwrap());
                    return Some(String::from_utf8(header.value.to_vec()).unwrap());
                }
            }
            None
        } else if req.is_partial() {
            for header in headers.iter() {
                if header.name == "Cookie" || header.name == "cookie" {
                    //println!("Host -> {}", String::from_utf8(header.value.to_vec()).unwrap());
                    return Some(String::from_utf8(header.value.to_vec()).unwrap());
                }
            }
            None
        } else {
            None
        }
    }
    */
    fn get_response<'a>(&self, data: &'a Vec<u8>) -> RelayedResponse<'a> {
    
        let mut headers = [httparse::EMPTY_HEADER; 128];
        let mut res = httparse::Response::new(&mut headers);
    
        let res_chk = res.parse(data);
    
        loop {
    
            if !res_chk.unwrap().is_complete() {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
    
            let reason = match res.reason {
                Some(r) => Some(r.to_string()),
                None => None,
            };
    
            let header_vec = res.headers.to_vec();
    
            let body = self.get_http_body(&data, header_vec).unwrap_or(String::from("[-] Failed to get http response body!").into_bytes());
            let string_body = String::from_utf8(body).unwrap();
    
            return RelayedResponse {
                http_version: res.version,
                http_code: res.code,
                http_reason: reason,
                http_headers: Some(headers.to_vec()),
                http_body: Some(string_body),
            };
        }
    }
    
    fn get_request<'a>(&self, data: &'a Vec<u8>) -> RelayRequest<'a> {
    
        let mut headers = [httparse::EMPTY_HEADER; 128];
        let mut req = httparse::Request::new(&mut headers);
    
        let req_chk = req.parse(data);
    
        loop {
    
            if !req_chk.unwrap().is_complete() {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
    
            let method = match req.method {
                Some(r) => Some(r.to_string()),
                None => None,
            };
    
            let path = match req.path {
                Some(p) => Some(p.to_string()),
                None => None,
            };
    
            let header_vec = req.headers.to_vec();
    
            let body = self.get_http_body(&data, header_vec).unwrap_or(String::from("[-] Failed to get http request body!").into_bytes());
            let string_body = String::from_utf8(body).unwrap();
    
            return RelayRequest {
                http_method: method,
                http_path: path,
                http_version: req.version,
                http_headers: Some(headers.to_vec()),
                http_body: Some(string_body),
            };
        }
    }
    
    fn get_http_body(&self, data: &Vec<u8>, headers: Vec<Header>) -> Option<Vec<u8>> {
    
        let mut cl: u64 = 0;
        let encoding_check = headers.clone();
        for header in headers {
    
            if header.name == "Content-Length" || header.name == "content-length" {
                let length = match String::from_utf8(header.value.to_vec()) {
                    Ok(s) => s,
                    Err(e) => {
                        println!("[!] ERROR: {}",e);
                        return None;
                    },
                };
    
                cl += length.parse::<u64>().unwrap();
                let mut r_body = data.to_vec();
                r_body.reverse();
                r_body.truncate(cl as usize);
                r_body.reverse();
    
                return Some(r_body);
    
            } else if header.name == "Transfer-Encoding" || header.name == "transfer-encoding" {
    
                let te_string = String::from_utf8(header.value.to_vec()).unwrap();
    
                if te_string == "chunked" {
    
                    let mut new_vec = data.to_vec();
                    let new_vec_iter = new_vec.to_vec();
                    let mut new_vec_iter = new_vec_iter.iter();
                    let mut i = 0;
    
                    loop {
                        if let Some(&0x0d) = new_vec_iter.next() {
                            i += 1;
                            if let Some(&0x0a) = new_vec_iter.next() {
                                i += 1;
                                if let Some(&0x0d) = new_vec_iter.next() {
                                    i += 1;
                                    if let Some(&0x0a) = new_vec_iter.next() {
                                        i += 1;
                                        break;
                                    } else {i += 1;}
                                } else {i += 1;}
                            } else {i += 1;}
                        } else {i += 1;}
                    }
    
                    new_vec.reverse();
                    new_vec.truncate(data.len() - i);
                    new_vec.reverse();
    
                    let mut decoder = Decoder::new(new_vec.as_slice());
                    let mut blob = Vec::new();
                    let _ = decoder.read_to_end(&mut blob);
    
                    let mut g_encoded = false;
                    for header in encoding_check {
                        if header.name == "Content-Encoding" || header.name == "content-encoding" {
                            let s = String::from_utf8(header.value.to_vec()).unwrap();
                            if s == "gzip" {
                                g_encoded = true;
                            }
                        }
                    }
                    if g_encoded {
                        let mut gzd = GzDecoder::new(&blob[..]);
                        let mut unzipped = Vec::new();
                        match gzd.read_to_end(&mut unzipped) {
                            Ok(_) => {},
                            Err(_) => return None,
                        }
        
                        return Some(unzipped);
                    } else {
                        return Some(blob);
                    }
    
                }
            }
        }
        return Some(String::from("None").into_bytes());
    }
} // DataHandler


/* Rewrite this to handle TCP connections until TCP connection is dropped instead of dropping it */
fn handle_stream<H: HandlerCallbacks + std::marker::Sync + std::marker::Send + 'static>(tcp_stream: SslStream<TcpStream>, rc_config: Arc<Mutex<RelayConfig>>, handlers: Arc<Mutex<H>>) {

    let conf_lock = rc_config.lock().unwrap();
    let remote_host = format!("{}:{}", conf_lock.remote_host, conf_lock.remote_port);
    let verbose_mode = conf_lock.verbose_level;
    drop(conf_lock);

    let mut datahandler = DataHandler::new(tcp_stream, remote_host);

    let mut data = Vec::<u8>::new();
    let mut response_data = Vec::<u8>::new();

    let data_size = datahandler.get_data_stream(&mut data);
    if data_size == 0 {
        println!("[!] Got 0 bytes closing tcp stream!");
        return;
    }
    if verbose_mode == 1 {
        datahandler.http_req_verbose(&data, 1);
    } else if verbose_mode == 2 {
        datahandler.http_req_verbose(&data, 2);
    }

    let handlers_p = handlers.clone();
    let d = data.clone();

    thread::spawn(move || {
        let handlers_lock = handlers_p.lock().unwrap();
        handlers_lock.req_nb_callback(d);
        drop(handlers_lock);
    });

    let handlers_p = handlers.clone();
    let handlers_lock = handlers_p.lock().unwrap();
    handlers_lock.req_b_callback(&mut data);
    drop(handlers_lock);



    match datahandler.relay_data(&data) {
        Some(_relay_success) => {},
        None => {
            println!("[-] relay_data failed!");
            return;
        }
    }

    // Get Upstream Data
    datahandler.stream_direction = StreamDirection::Upstream;
    let _response_size = datahandler.get_data_stream(&mut response_data);

    if verbose_mode == 1 {
        datahandler.http_res_verbose(&response_data, 1);
    } else if verbose_mode == 2 {
        datahandler.http_res_verbose(&response_data, 2);
    }

    // Switch back to DownStream mode to relay data from remote host back to origin host
    datahandler.stream_direction = StreamDirection::DownStream;

    let handlers_p = handlers.clone();
    let d = response_data.clone();

    thread::spawn(move || {
        let handlers_lock = handlers_p.lock().unwrap();
        handlers_lock.res_nb_callback(d);
        drop(handlers_lock);
    });

    let handlers_p = handlers.clone();
    let handlers_lock = handlers_p.lock().unwrap();
    handlers_lock.res_b_callback(&mut response_data);
    drop(handlers_lock);

    datahandler.tcp_stream.as_mut().unwrap().write_all(&response_data).unwrap();
    let _ = datahandler.tcp_stream.as_mut().unwrap().flush();
}