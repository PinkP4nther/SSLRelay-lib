# SSLRelay
A relay I wrote to help with intercepting/modifying TLS encrypted network traffic from an application.

The idea is to generate a certificate and a private key (You may need to generate a CA for your certificate, so that you can tell your system or the application to trust the generated certificate).
Then use this library to continuously rewrite or display encrypted network traffic.

Right now this library is mostly written to target the HTTP over TLS however I would like to make it work seamlessly with any data over TLS/SSL.