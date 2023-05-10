//web_app.ts



//Initiate WebSocket connection
function connect(url: string): WebSocket {
    console.log("connecting to " + url);
    let ws = new WebSocket(url); 
    ws.onopen = onOpen;
    ws.onerror = onError;
    ws.onclose = onClose;
    return ws;
}
//indicates that the connection is ready to send and receive data
function onOpen(event: any): void {
    console.log("connected");
//$("#btnConnect").html("Connected");
}

//An event listener to be called when an error occurs. This is a simple event named "error".
function onError(event: any): void {
    console.log(JSON.stringify(event.data));
}
//An event listener to be called when the WebSocket connection's readyState changes to CLOSED.
function onClose(event: any): void {
    console.log(JSON.stringify(event.data));
}

export { connect };