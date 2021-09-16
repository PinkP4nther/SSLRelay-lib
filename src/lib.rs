use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use std::net::{TcpListener};
use std::sync::{Arc, Mutex};
use std::{process, thread};
use std::env;
use std::fs;
use std::path::Path;

use toml::Value as TValue;

mod data;
use data::FullDuplexTcp;

#[derive(Clone)]
pub struct RelayConfig {
    pub bind_host: String,
    pub bind_port: String,
    pub remote_host: String,
    pub remote_port: String,
    pub ssl_private_key_path: String,
    pub ssl_cert_path: String,
}

pub trait HandlerCallbacks {
    fn ds_b_callback(&self, _in_data: Vec<u8>) -> CallbackRet {CallbackRet::Relay(_in_data)}
    fn ds_nb_callback(&self, _in_data: Vec<u8>){}
    fn us_b_callback(&self, _in_data: Vec<u8>) -> CallbackRet {CallbackRet::Relay(_in_data)}
    fn us_nb_callback(&self, _in_data: Vec<u8>){}
}

pub enum ConfigType<T> {
    Env,
    Path(T),
    Conf(RelayConfig),
    Default,
}

#[derive(Debug)]
pub enum CallbackRet {
    Relay(Vec<u8>),// Relay data
    Spoof(Vec<u8>),// Skip relaying and send data back
    Shutdown,// Shutdown TCP connection
    Freeze,// Dont send data (pretend as if stream never was recieved)
}

#[derive(Clone)]
pub struct SSLRelay<H>
where
    H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static,
{
    config: Option<RelayConfig>,
    handlers: Option<InnerHandlers<H>>,
}

#[derive(Clone)]
pub struct InnerHandlers<H>
where
    H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static,
{
    cb: H
}

impl<H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static> SSLRelay<H> {

    pub fn new(handlers: H) -> Self {

        SSLRelay {
            config: None,
            handlers: Some(InnerHandlers{cb: handlers}),
        }
    }

    pub fn load_config(&mut self, config_path: ConfigType<String>) {
        self.config = Some(self.load_relay_config(config_path));
    }

    pub fn start(&mut self) {

        let rc_pointer = Arc::new(Mutex::new(self.config.as_ref().unwrap().clone()));

        let rhost = rc_pointer.lock().unwrap().remote_host.clone();
        let rport = rc_pointer.lock().unwrap().remote_port.clone();
        let remote_endpoint = format!("{}:{}", rhost, rport);

        let acceptor = self.setup_ssl_config(self.config.as_ref().unwrap().ssl_private_key_path.clone(), self.config.as_ref().unwrap().ssl_cert_path.clone());
        let listener = TcpListener::bind(format!("{}:{}", self.config.as_ref().unwrap().bind_host.clone(), self.config.as_ref().unwrap().bind_port.clone())).unwrap();

        for stream in listener.incoming() {

            match stream {
                Ok(stream) => {

                    let acceptor = acceptor.clone();
                    //let rc_config = rc_pointer.clone();
                    let handler_clone = self.handlers.as_ref().unwrap().clone();
                    let r_endpoint = remote_endpoint.clone();

                    thread::spawn(move || {

                        match acceptor.accept(stream) {
                            Ok(stream) => {
                                // FULL DUPLEX OBJECT CREATION HERE
                                FullDuplexTcp::new(stream, r_endpoint, handler_clone).handle();
                            },
                            Err(e) => {

                                println!("[Error] {}", e);
                            }
                        }
                    });
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

        RelayConfig {
            bind_host: bind_host.clone(),
            bind_port: bind_port.clone(),
            ssl_private_key_path: ssl_private_key_path.clone(),
            ssl_cert_path: ssl_cert_path.clone(),
            remote_host: remote_host.clone(),
            remote_port: remote_port.clone(),
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