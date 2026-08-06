use once_cell::sync::Lazy;

pub struct Config {
    pub api_key: String,

    pub postgres_user: String,
    pub postgres_password: String,
    pub postgres_host: String,
    pub postgres_port: u32,
    pub postgres_db: String,

    pub sentry_dsn: Option<String>,

    pub token_enc_key: String,

    pub postgres_pool_max_connections: u32,
    pub postgres_pool_acquire_timeout_sec: u64,
    pub postgres_pool_max_lifetime_sec: u64,
    pub application_name: String,

    pub port: u16,
}

fn get_env(env: &'static str) -> String {
    std::env::var(env).unwrap_or_else(|_| panic!("Cannot get the {} env variable", env))
}

fn get_env_or(env: &'static str, default: &'static str) -> String {
    std::env::var(env).unwrap_or_else(|_| default.to_string())
}

impl Config {
    pub fn load() -> Config {
        Config {
            api_key: get_env("API_KEY"),

            postgres_user: get_env("POSTGRES_USER"),
            postgres_password: get_env("POSTGRES_PASSWORD"),
            postgres_host: get_env("POSTGRES_HOST"),
            postgres_port: get_env("POSTGRES_PORT")
                .parse()
                .unwrap_or_else(|_| panic!("Cannot parse the POSTGRES_PORT env variable")),
            postgres_db: get_env("POSTGRES_DB"),

            sentry_dsn: std::env::var("SENTRY_DSN").ok(),

            token_enc_key: get_env("TOKEN_ENC_KEY"),

            postgres_pool_max_connections: std::env::var("POSTGRES_POOL_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            postgres_pool_acquire_timeout_sec: std::env::var("POSTGRES_POOL_ACQUIRE_TIMEOUT_SEC")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            postgres_pool_max_lifetime_sec: std::env::var("POSTGRES_POOL_MAX_LIFETIME_SEC")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1800),
            application_name: get_env_or("APPLICATION_NAME", "services_manager_server"),

            port: std::env::var("PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8080),
        }
    }
}

pub static CONFIG: Lazy<Config> = Lazy::new(Config::load);
