# SSLRelay
A relay library I wrote to help with intercepting/modifying TLS encrypted network traffic from an application.

The idea is to generate a certificate and a private key (You may need to generate a CA for your certificate, so that you can tell your system or the application to trust the generated certificate).
Then use this library to continuously rewrite or display decrypted network traffic.

This library now supports continuous TCP sessions. | 09/02/2021\
Fixed stream threads becoming locked when an abrupt shutdown of the TCP stream occurs. | 09/13/2021

Fix race condition where streams coming from up/down stream at same time can cause desync of master and stream threads | TODO

More updates/ideas to come.. I think..