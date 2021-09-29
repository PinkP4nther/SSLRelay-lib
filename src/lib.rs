use openssl::ssl::{
    SslVerifyMode,
    SslConnector,
    SslAcceptor,
    SslStream,
    SslFiletype,
    SslMethod
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
    process,
    thread
};

use std::{
    env,
    fs,
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

use toml::Value as TValue;

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

#[derive(Copy, Clone)]
pub enum TCPDataType {
    TLS,
    RAW,
}

pub enum ConfigType<T> {
    Env,
    Path(T),
    Conf(RelayConfig),
    Default,
}

#[derive(Clone)]
pub struct RelayConfig {
    pub downstream_data_type: TCPDataType,
    pub upstream_data_type: TCPDataType,
    pub bind_host: String,
    pub bind_port: String,
    pub remote_host: String,
    pub remote_port: String,
    pub ssl_private_key_path: String,
    pub ssl_cert_path: String,
}

#[derive(Debug)]
pub enum CallbackRet {
    Relay(Vec<u8>),// Relay data
    Spoof(Vec<u8>),// Skip relaying and send data back
    Shutdown,// Shutdown TCP connection
    Freeze,// Dont send data (pretend as if stream never was recieved)
}

pub trait HandlerCallbacks {
    fn ds_b_callback(&self, _in_data: Vec<u8>) -> CallbackRet {CallbackRet::Relay(_in_data)}
    fn ds_nb_callback(&self, _in_data: Vec<u8>){}
    fn us_b_callback(&self, _in_data: Vec<u8>) -> CallbackRet {CallbackRet::Relay(_in_data)}
    fn us_nb_callback(&self, _in_data: Vec<u8>){}
}


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