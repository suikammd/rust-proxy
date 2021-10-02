simple proxy base on rust, which helps you bypass firewalls

goal:
1. practice rust

feat:
1. based on websocket
2. pooled websocket connection

client:
1. get socks5 connections from browser
2. do handshake, choose which method to use (only support no auth for now)
3. retrieve which addr browser wants to go, build a tls websocket with server, then send a addr packet to server
4. combine socks5 stream to websocket stream, websocket message is a data packet
5. after finish reading from sock5 stream, client will send a close packet to server

server:
1. parse addr packet and connect to addr
2. combine proxy stream with websocket stream
3. loop above steps until server dies

## Credit
- [@dyxushuai](https://github.com/dyxushuai): pool implementation