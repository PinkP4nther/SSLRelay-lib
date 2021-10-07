# SSLRelay
A relay library I wrote to help with intercepting/modifying TLS encrypted network traffic from an application.

The idea is to generate a certificate and a private key (You may need to generate a CA for your certificate, so that you can tell your system or the application to trust the generated certificate).
Then use this library to continuously rewrite or display decrypted network traffic.

09/02/2021 | This library now supports continuous TCP sessions.

09/13/2021 | Fixed stream threads becoming locked when an abrupt shutdown of the TCP stream occurs.

09/14/2021 | Race condition between UP/DOWN stream threads fixed. As well as major performance improvements!

09/16/2021 | Added Callback return types that give much more control over data.

09/16/2021 | Version 0.3

09/21/2021 | v0.3.1 | Fully supports IPv6.

09/28/2021 | v0.4.0 | New feature added: Stream data types. Can now set type of stream data TLS or RAW. And some performance improvements.

09/28/2021 | v0.4.1 | Code restructured and organized.

10/06/2021 | v0.4.2 | Added documentation.

10/07/2021 | v0.4.3 | Blocking callbacks now pass self as a mutable reference. This can allow the developer to create structures that can be accessed every stream write ONLY in the BLOCKING callback. The self object is refreshed per TCP connection. Separate TCP connections can not touch eachothers data.

More updates/ideas to come.. I think..