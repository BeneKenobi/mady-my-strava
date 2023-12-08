#[allow(unused_imports)]
// supress warning for `dotenv().ok()` only being used in non-test code
use dotenv::dotenv;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json;
use std::env;
use url::Url;
use urlencoding::encode;

#[derive(Debug, PartialEq)]
struct StravaConfig {
    client_id: u32,
    client_secret: String,
    refresh_token: Option<String>,
    redirect_uri: String,
    access_token: Option<String>,
    strava_url: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct RefreshResponse {
    refresh_token: String,
    access_token: String,
    token_type: String,
    expires_in: u32,
}

fn main() {
    let config = load_env_variables().unwrap();
    println!("{:?}", config);
    if config.refresh_token.is_none() {
        let auth_url = build_auth_url(&config);
        println!("{}", auth_url);
    }
    let new_config = refresh_strava_token(&config);
    println!("{:?}", new_config);
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

    let redirect_uri = match env::var("STRAVA_REDIRECT_URI") {
        Ok(value) => value,
        Err(_) => "http://localhost/".to_string(),
    };

    Ok(StravaConfig {
        client_id,
        client_secret,
        refresh_token,
        redirect_uri,
        access_token: None,
        strava_url: "https://www.strava.com".to_string(),
    })
}

fn build_auth_url(config: &StravaConfig) -> String {
    let encoded_redirect_uri = encode(&config.redirect_uri);
    format!("https://www.strava.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope=read,activity:read,activity:write", &config.client_id, encoded_redirect_uri)
}

fn refresh_strava_token(config: &StravaConfig) -> StravaConfig {
    let url = match Url::parse(format!("{}/oauth/token", config.strava_url).as_str()) {
        Ok(url) => url,
        Err(e) => panic!("Failed to parse Strava URL: {}", e),
    };
    let data = [
        ("client_id", config.client_id.to_string()),
        ("client_secret", config.client_secret.clone()),
        ("refresh_token", config.refresh_token.clone().unwrap()),
        ("grant_type", "refresh_token".to_string()),
    ];

    let client = Client::new();
    let response = match client.post(url).form(&data).send() {
        Ok(response) => response,
        Err(e) => panic!("Failed to send request: {}", e),
    };

    if response.status() == 200 {
        let body = match response.text() {
            Ok(body) => body,
            Err(e) => panic!("Failed to read response body: {}", e),
        };
        let json: RefreshResponse = match serde_json::from_str(&body) {
            Ok(json) => json,
            Err(e) => panic!("Failed to parse JSON: {}", e),
        };
        StravaConfig {
            client_id: config.client_id,
            client_secret: config.client_secret.clone(),
            refresh_token: Some(json.refresh_token),
            redirect_uri: config.redirect_uri.clone(),
            access_token: Some(json.access_token),
            strava_url: config.strava_url.clone(),
        }
    } else {
        panic!("Failed to refresh token: {}", response.status());
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
            redirect_uri: "http://localhost/".to_string(),
            access_token: None,
            strava_url: "https://www.strava.com".to_string(),
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
            redirect_uri: "http://localhost/".to_string(),
            access_token: None,
            strava_url: "https://www.strava.com".to_string(),
        };

        assert_eq!(load_env_variables().unwrap(), expected);
    }
}

#[cfg(test)]
mod build_auth_url_tests {
    use super::*;

    #[test]
    fn test_build_auth_url() {
        let client_id: u32 = 123456;

        let config = StravaConfig {
            client_id: client_id,
            client_secret: "dummy_secret".to_string(),
            refresh_token: None,
            redirect_uri: "http://localhost/".to_string(),
            access_token: None,
            strava_url: "https://www.strava.com".to_string(),
        };

        let expected_url = format!("https://www.strava.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope=read,activity:read,activity:write", client_id, "http%3A%2F%2Flocalhost%2F");
        let actual_url = build_auth_url(&config);
        assert_eq!(expected_url, actual_url);
    }
}

#[cfg(test)]
mod refresh_strava_token_tests {
    use super::*;
    use mockito::Matcher;

    #[test]
    fn test_refresh_strava_token() {
        let client_id: u32 = 123456;
        let client_secret = "dummy_secret".to_string();
        let refresh_token = "dummy_token".to_string();
        let redirect_uri = "http://localhost/".to_string();

        let mut server = mockito::Server::new();

        let config = StravaConfig {
            client_id: client_id,
            client_secret: client_secret.clone(),
            refresh_token: Some(refresh_token.clone()),
            redirect_uri: redirect_uri.clone(),
            access_token: None,
            strava_url: server.url(),
        };

        let expected = StravaConfig {
            client_id: client_id,
            client_secret: client_secret.clone(),
            refresh_token: Some(refresh_token.clone()),
            redirect_uri: redirect_uri.clone(),
            access_token: Some("dummy_access_token".to_string()),
            strava_url: server.url(),
        };

        let mock = server.mock("POST", "/oauth/token")
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body(
                Matcher::AllOf(vec![Matcher::UrlEncoded("client_id".to_string(), "123456".to_string()), Matcher::UrlEncoded("client_secret".to_string(), "dummy_secret".to_string()), Matcher::UrlEncoded("refresh_token".to_string(), "dummy_token".to_string()), Matcher::UrlEncoded("grant_type".to_string(), "refresh_token".to_string())])
            )
            .with_status(200)
            .with_body(r#"{"refresh_token":"dummy_token","access_token":"dummy_access_token","token_type":"Bearer","expires_in":21600}"#)
            .create();
        assert_eq!(refresh_strava_token(&config), expected);
        mock.assert();
    }
}
