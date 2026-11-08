// Match the page's transport: local HTTP uses ws, HTTPS deployments use wss.
htmx.createWebSocket = function (url) {
    const address = new URL(url);
    address.protocol = location.protocol === "https:" ? "wss:" : "ws:";
    const socket = new WebSocket(address.href, []);
    socket.binaryType = htmx.config.wsBinaryType;
    return socket;
};
