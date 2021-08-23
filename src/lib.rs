use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod, SslStream};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::{process, thread};
use std::env;
use std::fs;
use std::path::Path;

use toml::Value as TValue;

mod data;
use data::{DataHandler, StreamDirection};

mod http;
use http as http_helper;

#[derive(Clone)]
pub struct RelayConfig {
    pub bind_host: String,
    pub bind_port: String,
    pub remote_host: String,
    pub remote_port: String,
    pub ssl_private_key_path: String,
    pub ssl_cert_path: String,
    pub verbose_level: i8,
}

pub trait HandlerCallbacks {
    fn ds_b_callback(&self, _in_data: &mut Vec<u8>){}
    fn ds_nb_callback(&self, _in_data: Vec<u8>){}
    fn us_b_callback(&self, _in_data: &mut Vec<u8>){}
    fn us_nb_callback(&self, _in_data: Vec<u8>){}
}

pub enum ConfigType<T> {
    Env,
    Path(T),
    Conf(RelayConfig),
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
            ConfigType::Conf(conf) => {
                return conf;
            }
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
        
        if !Path::new(priv_key.as_str()).exists() {
            println!("[-] [{}] does not exist!", priv_key);
            process::exit(-1);
        } else if !Path::new(cert.as_str()).exists() {
            println!("[-] [{}] does not exist!", cert);
            process::exit(-1);
        }

        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_private_key_file(priv_key, SslFiletype::PEM).unwrap();
        acceptor.set_certificate_chain_file(cert).unwrap();
        acceptor.check_private_key().unwrap();
        Arc::new(acceptor.build())
    }
}// SSLRelay

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
        http_helper::http_req_verbose(&data, 1);
    } else if verbose_mode == 2 {
        http_helper::http_req_verbose(&data, 2);
    }

    let handlers_p = handlers.clone();
    let d = data.clone();

    thread::spawn(move || {
        let handlers_lock = handlers_p.lock().unwrap();
        handlers_lock.ds_nb_callback(d);
        drop(handlers_lock);
    });

    let handlers_p = handlers.clone();
    let handlers_lock = handlers_p.lock().unwrap();
    handlers_lock.ds_b_callback(&mut data);
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
        http_helper::http_res_verbose(&response_data, 1);
    } else if verbose_mode == 2 {
        http_helper::http_res_verbose(&response_data, 2);
    }

    // Switch back to DownStream mode to relay data from remote host back to origin host
    datahandler.stream_direction = StreamDirection::DownStream;

    let handlers_p = handlers.clone();
    let d = response_data.clone();

    thread::spawn(move || {
        let handlers_lock = handlers_p.lock().unwrap();
        handlers_lock.us_nb_callback(d);
        drop(handlers_lock);
    });

    let handlers_p = handlers.clone();
    let handlers_lock = handlers_p.lock().unwrap();
    handlers_lock.us_b_callback(&mut response_data);
    drop(handlers_lock);

    datahandler.tcp_stream.as_mut().unwrap().write_all(&response_data).unwrap();
    let _ = datahandler.tcp_stream.as_mut().unwrap().flush();
}