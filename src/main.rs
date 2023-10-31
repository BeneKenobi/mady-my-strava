#[allow(unused_imports)]
// supress warning for `dotenv().ok()` only being used in non-test code
use dotenv::dotenv;
use std::env;
use url::form_urlencoded::byte_serialize;

#[derive(Debug, PartialEq)]
struct StravaConfig {
    client_id: u32,
    client_secret: String,
    refresh_token: Option<String>,
}

fn load_env_variables() -> Result<StravaConfig, &'static str> {
    #[cfg(not(test))] // Only load .env variables if we are not running tests
    {
        dotenv().ok(); // Load .env variables
    }

    let client_id: u32 = match env::var("STRAVA_CLIENT_ID") {
        Ok(value) => value
            .parse::<u32>()
            .map_err(|_| "Invalid STRAVA_CLIENT_ID")?,
        Err(_) => return Err("STRAVA_CLIENT_ID not set"),
    };

    let client_secret = match env::var("STRAVA_CLIENT_SECRET") {
        Ok(value) => value,
        Err(_) => return Err("STRAVA_CLIENT_SECRET not set"),
    };

    let refresh_token: Option<String> = match env::var("STRAVA_REFRESH_TOKEN") {
        Ok(value) => Some(value),
        Err(_) => None,
    };

    Ok(StravaConfig {
        client_id,
        client_secret,
        refresh_token,
    })
}

fn build_auth_url(config: &StravaConfig) -> String {
    let encoded_redirect_uri = byte_serialize("http://localhost/".as_bytes()).collect::<String>();
    let url = format!("https://www.strava.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope=read,activity:read,activity:write", config.client_id, encoded_redirect_uri);
    url

    // def build_auth_url(client_id: str, redirect_uri: str) -> str:
    // params = {
    //     "client_id": client_id,
    //     "redirect_uri": redirect_uri,
    //     "response_type": "code",
    //     "scope": "read,activity:read,activity:write",
    // }
    // return f"https://www.strava.com/oauth/authorize?{urlencode(params)}"
}

fn main() {
    let config = load_env_variables().unwrap();
    println!("{:?}", config);
    if config.refresh_token.is_none() {
        let auth_url = build_auth_url(&config);
        println!("{}", auth_url);
    }
}

#[cfg(test)]
mod load_env_variables_tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    fn set_valid_env_vars() {
        env::set_var("STRAVA_CLIENT_ID", "123456");
        env::set_var("STRAVA_CLIENT_SECRET", "dummy_secret");
        env::set_var("STRAVA_REFRESH_TOKEN", "dummy_token");
    }

    #[test]
    #[serial(env)]
    fn test_load_env_variables_valid() {
        set_valid_env_vars();

        let expected = StravaConfig {
            client_id: 123456,
            client_secret: "dummy_secret".to_string(),
            refresh_token: Some("dummy_token".to_string()),
        };

        assert_eq!(load_env_variables().unwrap(), expected);
    }

    #[test]
    #[serial(env)]
    fn test_load_env_variables_invalid_client_id() {
        env::set_var("STRAVA_CLIENT_ID", "not_a_number");
        env::set_var("STRAVA_CLIENT_SECRET", "dummy_secret");
        env::set_var("STRAVA_REFRESH_TOKEN", "dummy_token");

        match load_env_variables() {
            Ok(_) => panic!("Expected an Err because STRAVA_CLIENT_ID is not a number"),
            Err(e) => assert_eq!(e, "Invalid STRAVA_CLIENT_ID"),
        }
    }

    #[test]
    #[serial(env)]
    fn test_load_env_variables_missing_necessary_keys() {
        let keys_and_expected_errors = [
            ("STRAVA_CLIENT_ID", "STRAVA_CLIENT_ID not set"),
            ("STRAVA_CLIENT_SECRET", "STRAVA_CLIENT_SECRET not set"),
        ];

        for (key, expected_error) in &keys_and_expected_errors {
            // Set all keys to valid values
            set_valid_env_vars();

            // Remove the env variable that we want to test
            env::remove_var(key);

            // Run the function and check that it returns the correct error
            match load_env_variables() {
                Ok(_) => panic!("Expected an Err because one of the keys is not set"),
                Err(e) => assert_eq!(e, *expected_error),
            }
        }
    }

    #[test]
    #[serial(env)]
    fn test_load_env_variables_missing_refresh_token() {
        set_valid_env_vars();

        env::remove_var("STRAVA_REFRESH_TOKEN");

        // Remove the env variable that we want to test
        let expected = StravaConfig {
            client_id: 123456,
            client_secret: "dummy_secret".to_string(),
            refresh_token: None,
        };

        assert_eq!(load_env_variables().unwrap(), expected);
    }
}

#[cfg(test)]
mod build_auth_url_tests {
    use super::*;
    use url::form_urlencoded::byte_serialize;

    #[test]
    fn test_build_auth_url() {
        let client_id: u32 = 123456;

        let config = StravaConfig {
            client_id: client_id,
            client_secret: "dummy_secret".to_string(),
            refresh_token: None,
        };

        let encoded_redirect_uri =
            byte_serialize("http://localhost/".as_bytes()).collect::<String>();

        let expected_url = format!("https://www.strava.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope=read,activity:read,activity:write", client_id, encoded_redirect_uri);
        let actual_url = build_auth_url(&config);
        assert_eq!(expected_url, actual_url);
    }
}
