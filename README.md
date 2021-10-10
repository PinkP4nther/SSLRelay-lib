# SSLRelay

> Documentation: [sslrelay - docs.rs](https://docs.rs/sslrelay)
> Crate: [sslrelay - crates.io](https://crates.io/crates/sslrelay)

## Summary

A TCP relay library that can handle raw TCP and SSL/TLS connections. You can read and write the data everytime a data stream is received whether it comes from up stream or down stream. To write a callback you must implement the **HandlerCallbacks** trait into your handler struct. Your handler struct may also include fields you can **READ** in the non blocking callbacks, and **READ AND WRITE** in the blocking callbacks.

## Patch Notes

> 09/02/2021 | This library now supports continuous TCP sessions.
> 
> 09/13/2021 | Fixed stream threads becoming locked when an abrupt shutdown of the TCP stream occurs.
> 
> 09/14/2021 | Race condition between UP/DOWN stream threads fixed. As well as major performance improvements!
> 
> 09/16/2021 | Added Callback return types that give much more control over  data.
> 
> 09/16/2021 | Version 0.3
> 
> 09/21/2021 | **v0.3.1** | Fully supports IPv6.
> 
> 09/28/2021 | **v0.4.0** | New feature added: Stream data types. Can now set type of stream data TLS or RAW. And some performance improvements.
> 
> 09/28/2021 | **v0.4.1** | Code restructured and organized.
> 
> 10/06/2021 | **v0.4.2** | Added documentation.
>
> 10/07/2021 | **v0.4.3** | Blocking callbacks now pass self as a mutable reference. This can allow the developer to create structures that can be accessed every stream write. (ONLY in the BLOCKING callback). The self object is refreshed per TCP connection. Separate TCP connections can not touch eachothers data.
>
> 10/08/2021 | **v0.4.4** | Added ability to set TLS certificate and certificate private key to nothing. When passing RelayConfig to the relay object use the 'None' variant of Option<T> enum. In a config file put the cert path and key path as an empty 'String' to specify 'None'.
>
> 10/09/2021 | **v0.6.0** | Gone away with 'ConfigType<T>'! No more specifying config files unless the developer implements it themself. A new config enum 'TLSConfig' has been introduced. This has 3 variants FILE(Specify the cert and pk file paths), DATA(Pass the cert(PEM) data and the pk(PEM) data as bytes), NONE(This is when you are not using TLS on the listening/downstream side of the relay).