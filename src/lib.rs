use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use std::net::{TcpListener};
use std::sync::Arc;
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
    config: RelayConfig,
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

    pub fn new(handlers: H, config_path: ConfigType<String>) -> Self {

        SSLRelay {
            config: Self::load_relay_config(config_path),
            handlers: Some(InnerHandlers{cb: handlers}),
        }
    }

    pub fn start(&mut self) {

        /* For UpStream and DownStream TCP data type separation. It should probably start here.
         * start() will setup anything that needs to be setup before starting the listener.
         * Once everything is initialized it will then call another handle function that is
         * decided based upon the DS/US options.
         * Basically this method is decided from down stream options
         * If DS is set to RAW mode then it will call the RAW TCP data mode handler vice versa.
         * The stream type will be wrapped in a mode type.
         * DataStreamType::TLS(SslStream<TcpStream>) or DataStreamType::RAW(TcpStream)
         * This will be passed into the Full Duplex TCP simulator object and those
         * methods within will decide which handler is called for each specific stream.
         * WILL ONLY DECIDE HANDLER IF CANT DO GENERIC DATA TYPING WITH TRAITS
         * The remote / UpStream data type will be decided by another DataStreamType variant and passed separately
         * into the Full Duplex TCP simulator.

         * Maybe streams passed into FDTCP simulator object could be typed as Trait requirements instead of a strict type?
         * Like Write/Read etc.
        */

        let rhost = self.config.remote_host.clone();
        let rport = self.config.remote_port.clone();
        let remote_endpoint = format!("{}:{}", rhost, rport);

        let acceptor = self.setup_ssl_config(self.config.ssl_private_key_path.clone(), self.config.ssl_cert_path.clone());
        let listener = TcpListener::bind(format!("{}:{}", self.config.bind_host.clone(), self.config.bind_port.clone())).unwrap();

        for stream in listener.incoming() {

            match stream {
                Ok(stream) => {

                    let acceptor = acceptor.clone();
                    let handler_clone = self.handlers.as_ref().unwrap().clone();
                    let r_endpoint = remote_endpoint.clone();

                    let r_host = rhost.clone();
                    let r_port = rport.clone();

                    thread::spawn(move || {

                        match acceptor.accept(stream) {
                            Ok(stream) => {
                                // FULL DUPLEX OBJECT CREATION HERE
                                FullDuplexTcp::new(stream, r_host, r_port, handler_clone).handle();
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

    fn load_relay_config(config_path: ConfigType<String>) -> RelayConfig {

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