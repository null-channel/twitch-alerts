use crate::types::ConnectionMap;

pub struct FrontendApi {
    pub host_info: HostInfo,
    pub connection_state: ConnectionMap,
    pub asset_path: String,
}

#[derive(Clone)]
pub struct HostInfo {
    pub websocket_host: String,
    pub ws_port: u16,
    pub http_port: u16,
}

impl HostInfo {
    pub fn get_http_address(&self) -> String {
        format!("{}:{}", "0.0.0.0", self.http_port)
    }

    pub fn get_ws_address(&self) -> String {
        format!("{}:{}", "0.0.0.0", self.ws_port)
    }

    pub fn get_frontend_ws_address(&self) -> String {
        format!("{}:{}", self.websocket_host, self.ws_port)
    }
}
