input: listen_addr(server listens at this addr), proxy_addr(server will proxy request to this addr)

1. accept socks5 connection
2. do handshake: decide which method to use, only support no auth for now
    // browser request
    VER	NMETHODS	METHODS
    1	1	1-255

    // server response
    VER	METHOD
    1	1
3. after decide which method to use, browser will tell server which addr it wants to connect
    // browser request
    | VER | CMD | RSV  | ATYP | DST.ADDR | DST.PORT |
    | --- | --- | ---- | ---- | -------- | -------- |
    | 1   | 1   | 0x00 | 1    | 动态     | 2        |


    server will convert this request to a customized struct to a proxy server
    after then, if success, server response success rep code, if not, server will say fail and close this connection
    | VER | REP | RSV  | ATYP | DST.ADDR | DST.PORT |
    | --- | --- | ---- | ---- | -------- | -------- |
    | 1   | 1   | 0x00 | 1    | 动态     | 2        |

    
    then, server will combine request stream with proxy stream

