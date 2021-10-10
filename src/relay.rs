//! SSLRelay

use crate::{
    SSLRelay,
    HandlerCallbacks,
    InnerHandlers,
    TCPDataType,
    TcpListener,
    thread,
    FullDuplexTcp,
    DataStreamType,
    RelayConfig,
    Arc,
    SslAcceptor,
    Path,
    SslMethod,
    SslFiletype,
    TLSConfig,
    PKey,
    X509,
};

impl<H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static> SSLRelay<H> {
    /// Creates new SSLRelay instance.
    pub fn new(handlers: H, config: RelayConfig) -> Self {

        SSLRelay {
            config,
            handlers: Some(InnerHandlers{cb: handlers}),
        }
    }
    /// Starts the SSLRelay connection handling.
    pub fn start(&mut self) {

        let rhost = self.config.remote_host.clone();
        let rport = self.config.remote_port.clone();
        let upstream_data_stream_type = self.config.upstream_data_type;

        let listener = TcpListener::bind(format!("{}:{}", self.config.bind_host.clone(), self.config.bind_port.clone())).unwrap();

        match self.config.downstream_data_type {

            TCPDataType::TLS => {
    
                let acceptor = self.setup_ssl_config(self.config.tls_config.clone());

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
                                           Err(_ec) => {println!("[SSLRelay Error] Failed to handle TCP(TLS) connection: {}", _ec)}
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
                                    Err(_ec) => println!("[SSLRelay Error] Failed to handle TCP connection: {}", _ec),
                                }
                            });
                        },
                        Err(e) => {println!("[Error] Tcp Connection Failed: {}", e)}
                    }
                }
            }
        }
    }

    fn setup_ssl_config(&self, tls_config: TLSConfig) -> Arc<SslAcceptor> {

        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();

        match tls_config {
            TLSConfig::FILE{certificate_path, private_key_path} => {

                if !Path::new(&private_key_path).exists() {
                    panic!("[-] [{}] does not exist!", private_key_path);
                }
                if !Path::new(&certificate_path).exists() {
                    panic!("[-] [{}] does not exist!", certificate_path);
                }
                acceptor.set_private_key_file(private_key_path, SslFiletype::PEM).unwrap();
                acceptor.set_certificate_chain_file(certificate_path).unwrap();
                acceptor.check_private_key().unwrap();
            },
            TLSConfig::DATA{certificate, private_key} => {
                let x_509_certificate = X509::from_pem(certificate.as_slice()).unwrap();
                let private_key = PKey::private_key_from_pem(private_key.as_slice()).unwrap();
                acceptor.set_certificate(x_509_certificate.as_ref()).unwrap();
                acceptor.set_private_key(private_key.as_ref()).unwrap();
            },
            TLSConfig::NONE => {
                panic!("[SSLRelay Error] Specified NONE for TLSConfig and downstream_data_type as TLS.");
            }
        }
        Arc::new(acceptor.build())
    }
}// SSLRelay