no encryption for now

1. Get connection from server
2. try parse to get which addr browser wants
    // server forwarded request with few modifications for now (will define a custom protocol in the future, now is simple forward)
    | CMD | ATYP | DST.ADDR | DST.PORT |
    | --- | ---- | -------- | -------- |
    | 1   |  1    | 动态     | 2        |


    client will connect to these specified addr & port
    then, client will combine request stream and proxy stream together