proxy helps you bypass firewalls

goal:
1. practice rust

feat:
1. based on websocket

server:
1. get socks5 connections from browser
2. do handshake, choose which method to use (only support no auth for now)
3. retrieve which addr browser wants to go, and convert to a custom request to client(no matter success or not, says ok to browser).
4. convert tcp stream to websocket stream and combine this stream to proxy stream

client:
1. get and parse custom request from websocket stream
2. connect to addr in custom request and get a new stream
3. combine proxy stream with transformed new stream (websocket based)