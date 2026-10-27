use once_cell::sync::Lazy;

pub struct Config {
    pub api_key: String,

    pub postgres_user: String,
    pub postgres_password: String,
    pub postgres_host: String,
    pub postgres_port: u16,
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

fn get_env_parsed<T: std::str::FromStr>(env: &'static str, default: T) -> T {
    match std::env::var(env) {
        Ok(v) => v
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("Cannot parse the {} env variable: {:?}", env, v)),
        Err(_) => default,
    }
}

const MIN_API_KEY_LEN: usize = 16;

fn validate_api_key(key: String) -> String {
    let key = key.trim().to_string();
    if key.len() < MIN_API_KEY_LEN {
        panic!(
            "API_KEY must be set and at least {} characters long",
            MIN_API_KEY_LEN
        );
    }
    key
}

impl Config {
    pub fn load() -> Config {
        Config {
            api_key: validate_api_key(get_env("API_KEY")),

            postgres_user: get_env("POSTGRES_USER"),
            postgres_password: get_env("POSTGRES_PASSWORD"),
            postgres_host: get_env("POSTGRES_HOST"),
            postgres_port: get_env("POSTGRES_PORT")
                .parse()
                .unwrap_or_else(|_| panic!("Cannot parse the POSTGRES_PORT env variable as u16")),
            postgres_db: get_env("POSTGRES_DB"),

            sentry_dsn: std::env::var("SENTRY_DSN").ok(),

            token_enc_key: get_env("TOKEN_ENC_KEY"),

            postgres_pool_max_connections: get_env_parsed("POSTGRES_POOL_MAX_CONNECTIONS", 10),
            postgres_pool_acquire_timeout_sec: get_env_parsed(
                "POSTGRES_POOL_ACQUIRE_TIMEOUT_SEC",
                5,
            ),
            postgres_pool_max_lifetime_sec: get_env_parsed("POSTGRES_POOL_MAX_LIFETIME_SEC", 1800),
            application_name: get_env_or("APPLICATION_NAME", "services_manager_server"),

            port: get_env_parsed("PORT", 8080),
        }
    }
}

pub static CONFIG: Lazy<Config> = Lazy::new(Config::load);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "API_KEY")]
    fn empty_api_key_rejected() {
        validate_api_key("   ".to_string());
    }

    #[test]
    #[should_panic(expected = "API_KEY")]
    fn short_api_key_rejected() {
        validate_api_key("short".to_string());
    }

    #[test]
    fn long_api_key_accepted() {
        assert_eq!(
            validate_api_key("0123456789abcdef".to_string()),
            "0123456789abcdef"
        );
    }
}
