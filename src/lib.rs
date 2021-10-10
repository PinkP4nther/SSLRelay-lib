//! ## SSLRelay

//! Library for relaying TCP traffic as well as TLS encrypted TCP traffic.
//! This library allows you to implement callback functions for upstream and downstream traffic.
//! These callbacks can R/W the data from a stream(Blocking) or only R the data(Non-Blocking).
//!```
//!pub trait HandlerCallbacks {
//!    fn ds_b_callback(&mut self, _in_data: Vec<u8>) -> CallbackRet {CallbackRet::Relay(_in_data)}
//!    fn ds_nb_callback(&self, _in_data: Vec<u8>){}
//!    fn us_b_callback(&mut self, _in_data: Vec<u8>) -> CallbackRet {CallbackRet::Relay(_in_data)}
//!    fn us_nb_callback(&self, _in_data: Vec<u8>){}
//!}
//!```
//! The blocking callbacks return an enum called CallbackRet with four different variants.
//! The variants control the flow of the tcp stream.
//!```
//! pub enum CallbackRet {
//!     Relay(Vec<u8>),// Relay data
//!     Spoof(Vec<u8>),// Skip relaying and send data back
//!     Shutdown,// Shutdown TCP connection
//!     Freeze,// Dont send data (pretend as if stream never was recieved)
//! }
//! ```
//! ## Example (basic.rs)
//! ```
//! use sslrelay::{self, RelayConfig, HandlerCallbacks, CallbackRet, TCPDataType, TLSConfig};
//! 
//! // Handler object
//! #[derive(Clone)] // Must have Clone trait implemented.
//! struct Handler;
//! 
//! /*
//!     Callback traits that can be used to read or inject data
//!     into data upstream or downstream.
//! */
//! impl HandlerCallbacks for Handler {
//! 
//!     // DownStream non blocking callback (Read Only)
//!     fn ds_nb_callback(&self, _in_data: Vec<u8>) {
//!         println!("[CALLBACK] Down Stream Non Blocking CallBack!");
//!     }
//! 
//!     // DownStream blocking callback (Read & Write)
//!     fn ds_b_callback(&mut self, _in_data: Vec<u8>) -> CallbackRet {
//!         println!("[CALLBACK] Down Stream Blocking CallBack!");
//!         CallbackRet::Relay(_in_data)
//!     }
//! 
//!     // UpStream non blocking callback (Read Only)
//!     fn us_nb_callback(&self, _in_data: Vec<u8>) {
//!         println!("[CALLBACK] Up Stream Non Blocking CallBack!");
//!     }
//! 
//!     // UpStream blocking callback (Read & Write)
//!     fn us_b_callback(&mut self, _in_data: Vec<u8>) -> CallbackRet {
//!         println!("[CALLBACK] Up Stream Blocking CallBack!");
//!         CallbackRet::Relay(_in_data)
//!     }
//! }
//! 
//! fn main() {
//! 
//!     // Create new SSLRelay object
//!     let mut relay = sslrelay::SSLRelay::new(
//!         Handler, 
//!         RelayConfig {
//!             downstream_data_type: TCPDataType::TLS,
//!             upstream_data_type: TCPDataType::TLS,
//!             bind_host: "0.0.0.0".to_string(),
//!             bind_port: "443".to_string(),
//!             remote_host: "remote.com".to_string(),
//!             remote_port: "443".to_string(),
//!             tls_config: TLSConfig::FILE{
//!                 certificate_path: "./tls.crt".to_string(),
//!                 private_key_path: "./tls.key".to_string(),
//!             },
//!         }
//!     );
//! 
//!     // Start listening
//!     relay.start();
//! }
//! ```

use openssl::{
    x509::X509,
    pkey::PKey,
    ssl::{
        SslVerifyMode,
        SslConnector,
        SslAcceptor,
        SslStream,
        SslFiletype,
        SslMethod,
    }
};

use std::net::{
    TcpListener,
    TcpStream,
    Shutdown
};

use std::sync::{
    Arc,
    Mutex
};

use std::{
    thread
};

use std::{
    path::Path,
    time::Duration,
};

use std::io::{
    self,
    Read,
    Write
};

use std::sync::mpsc::{
    self,
    Receiver,
    Sender
};

mod data;
mod tcp;
mod relay;

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

enum DataStreamType {
    RAW(TcpStream),
    TLS(SslStream<TcpStream>),
}

/// Specifies the upstream or downstream data type (TLS or RAW).
#[derive(Copy, Clone)]
pub enum TCPDataType {
    TLS,
    RAW,
}

/// TLSConfig is used to specify TLS options.
/// FILE is for specifying a path to a certificate and private key.
/// DATA is for passing the certificate and private key bytes directly.
/// NONE is for when you are not using TLS on the listening/downstream side of the relay.
#[derive(Clone)]
pub enum TLSConfig {
    FILE {certificate_path: String, private_key_path: String},
    DATA {certificate: Vec<u8>, private_key: Vec<u8>},
    NONE,
}

/// Relay Config structure for passing into the SSLRelay::new() config parameter.
#[derive(Clone)]
pub struct RelayConfig {
    pub downstream_data_type: TCPDataType,
    pub upstream_data_type: TCPDataType,
    pub bind_host: String,
    pub bind_port: String,
    pub remote_host: String,
    pub remote_port: String,
    pub tls_config: TLSConfig,
}

/// CallbackRet for blocking callback functions
#[derive(Debug)]
pub enum CallbackRet {
    Relay(Vec<u8>),// Relay data
    Spoof(Vec<u8>),// Skip relaying and send data back
    Shutdown,// Shutdown TCP connection
    Freeze,// Dont send data (pretend as if stream never was recieved)
}

/// Callback functions a user may or may not implement.
pub trait HandlerCallbacks {
    fn ds_b_callback(&mut self, _in_data: Vec<u8>) -> CallbackRet {CallbackRet::Relay(_in_data)}
    fn ds_nb_callback(&self, _in_data: Vec<u8>){}
    fn us_b_callback(&mut self, _in_data: Vec<u8>) -> CallbackRet {CallbackRet::Relay(_in_data)}
    fn us_nb_callback(&self, _in_data: Vec<u8>){}
}

/// The main SSLRelay object.
#[derive(Clone)]
pub struct SSLRelay<H>
where
    H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static,
{
    config: RelayConfig,
    handlers: Option<InnerHandlers<H>>,
}

#[allow(dead_code)]
struct FullDuplexTcp<H>
where
    H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static,
{
    remote_host: String,
    remote_port: String,
    ds_inner_m: Arc<Mutex<Option<DownStreamInner>>>,
    us_inner_m: Arc<Mutex<Option<UpStreamInner>>>,
    inner_handlers: InnerHandlers<H>,
}

#[derive(Clone)]
struct InnerHandlers<H>
where
    H: HandlerCallbacks + std::marker::Sync + std::marker::Send + Clone + 'static,
{
    cb: H
}

struct DownStreamInner
{
    ds_stream: DataStreamType,
    internal_data_buffer: Vec<u8>,
}

struct UpStreamInner
{
    us_stream: DataStreamType,
    internal_data_buffer: Vec<u8>
}