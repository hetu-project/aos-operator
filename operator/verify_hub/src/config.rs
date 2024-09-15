#[derive(Debug)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug)]
pub struct DatabaseConfig {
    pub url: String,
}

impl Config {
    pub fn new() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
            },
            database: DatabaseConfig {
                url: "postgres://postgres:123456@127.0.0.1:5432/dispatcher".to_string(),
            },
        }
    }
}