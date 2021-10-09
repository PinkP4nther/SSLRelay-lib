//! SSLRelay

use crate::{
    SSLRelay,
    HandlerCallbacks,
    InnerHandlers,
    TCPDataType,
    ConfigType,
    TcpListener,
    thread,
    FullDuplexTcp,
    DataStreamType,
    RelayConfig,
    env,
    TValue,
    fs,
    process,
    Arc,
    SslAcceptor,
    Path,
    SslMethod,
    SslFiletype,
};

impl<H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static> SSLRelay<H> {
    /// Creates new SSLRelay instance.
    pub fn new(handlers: H, config: ConfigType<String>) -> Self {

        SSLRelay {
            config: Self::load_relay_config(config),
            handlers: Some(InnerHandlers{cb: handlers}),
        }
    }
    /// Starts the SSLRelay connection handling.
    pub fn start(&mut self) {

        let rhost = self.config.remote_host.clone();
        let rport = self.config.remote_port.clone();
        let listener = TcpListener::bind(format!("{}:{}", self.config.bind_host.clone(), self.config.bind_port.clone())).unwrap();
        let upstream_data_stream_type = self.config.upstream_data_type;

        match self.config.downstream_data_type {

            TCPDataType::TLS => {
                let acceptor = self.setup_ssl_config(self.config.ssl_private_key_path.clone(), self.config.ssl_cert_path.clone());

                for stream in listener.incoming() {
        
                    match stream {
                        Ok(stream) => {
        
                            let acceptor = acceptor.clone();
                            let handler_clone = self.handlers.as_ref().unwrap().clone();
        
                            let r_host = rhost.clone();
                            let r_port = rport.clone();
        
                            thread::spawn(move || {
        
                                match acceptor.accept(stream) {
                                    Ok(stream) => {
                                        // FULL DUPLEX OBJECT CREATION HERE
                                       match FullDuplexTcp::new(DataStreamType::TLS(stream), upstream_data_stream_type, r_host, r_port, handler_clone) {
                                           Ok(mut fdtcp) => fdtcp.handle(),
                                           Err(_ec) => {}
                                       }
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
            },

            TCPDataType::RAW => {

                for stream in listener.incoming() {
        
                    match stream {
                        Ok(stream) => {
        
                            let handler_clone = self.handlers.as_ref().unwrap().clone();
        
                            let r_host = rhost.clone();
                            let r_port = rport.clone();
        
                            thread::spawn(move || {
    
                                // FULL DUPLEX OBJECT CREATION HERE
                                match FullDuplexTcp::new(DataStreamType::RAW(stream), upstream_data_stream_type, r_host, r_port, handler_clone) {
                                    Ok(mut fdtcp) => fdtcp.handle(),
                                    Err(_ec) => {},
                                }

                            });
                        },
                        Err(e) => {println!("[Error] Tcp Connection Failed: {}", e)}
                    }
                }
            }
        }
    }

    fn load_relay_config(config: ConfigType<String>) -> RelayConfig {

        let mut resolved_path = String::from("./relay_config.toml");
        match config {
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
        let upstream_tls_conf = config_parsed["upstream_data_type"].to_string().replace("\"", "").to_lowercase();
        let downstream_tls_conf = config_parsed["downstream_data_type"].to_string().replace("\"", "").to_lowercase();

        let upstream_data_type: TCPDataType;
        let downstream_data_type: TCPDataType;

        if upstream_tls_conf == "tls" {
            upstream_data_type = TCPDataType::TLS;
        } else if upstream_tls_conf == "raw" {
            upstream_data_type = TCPDataType::RAW;
        } else {
            panic!("[SSLRelay Error] Unrecognized TCPDataType for upstream_data_type. Data type received was not 'tcp' or 'tls'!");
// Create error handling for load_relay_config()
        }

        if downstream_tls_conf == "tls" {
            downstream_data_type = TCPDataType::TLS;
        } else if downstream_tls_conf == "raw" {
            downstream_data_type = TCPDataType::RAW;
        } else {
            panic!("[SSLRelay Error] Unrecognized TCPDataType for downstream_data_type. Data type received was not 'tcp' or 'tls'!");
// Create error handling for load_relay_config()
        }

        let mut ssl_pk_path = None;
        let mut ssl_c_path = None;

        if !ssl_private_key_path.is_empty() {
            ssl_pk_path = Some(ssl_private_key_path.clone());
        }

        if !ssl_cert_path.is_empty() {
            ssl_c_path = Some(ssl_cert_path.clone());
        }

        RelayConfig {
            upstream_data_type,
            downstream_data_type,
            bind_host: bind_host.clone(),
            bind_port: bind_port.clone(),
            ssl_private_key_path: ssl_pk_path,
            ssl_cert_path: ssl_c_path,
            remote_host: remote_host.clone(),
            remote_port: remote_port.clone(),
        }
    }

    fn setup_ssl_config(&self, priv_key: Option<String>, cert: Option<String>) -> Arc<SslAcceptor> {

        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();

        let private_key = priv_key.expect("[SSLRelay Error] No private key file specified!");
        let certificate = cert.expect("[SSLRelay Error] No certificate file specified!");
        
        if !Path::new(&private_key).exists() {
            panic!("[-] [{}] does not exist!", private_key);
        }
        acceptor.set_private_key_file(private_key, SslFiletype::PEM).unwrap();

        if !Path::new(&certificate).exists() {
            panic!("[-] [{}] does not exist!", certificate);
        }
        acceptor.set_certificate_chain_file(certificate).unwrap();

        acceptor.check_private_key().unwrap();
        Arc::new(acceptor.build())
    }
}// SSLRelay