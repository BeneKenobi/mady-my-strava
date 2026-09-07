#[allow(unused_imports)]
// supress warning for `dotenv().ok()` only being used in non-test code
use dotenv::dotenv;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use std::env;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;
use urlencoding::encode;

/// Sport type of a regular (non-electric) bike ride on Strava.
const RIDE_SPORT_TYPE: &str = "Ride";
/// Sport type an activity is changed to when the account wants e-bike rides.
const EBIKE_SPORT_TYPE: &str = "EBikeRide";
/// How far back activities are looked at, in seconds.
const LOOKBACK_SECONDS: u64 = 24 * 60 * 60;
/// File the rotated refresh tokens are written back to.
const ENV_FILE: &str = ".env";
/// Name of the `.env` key holding the account list.
const ACCOUNTS_KEY: &str = "STRAVA_ACCOUNTS";

/// One Strava account the tool acts on.
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
struct Account {
    name: String,
    refresh_token: String,
    /// Convert every regular ride of this account into an e-bike ride.
    #[serde(default)]
    force_ebike: bool,
    /// Only valid for the current run, so it is never written back to `.env`.
    #[serde(skip)]
    access_token: Option<String>,
}

#[derive(Debug, PartialEq)]
struct StravaConfig {
    client_id: u32,
    client_secret: String,
    redirect_uri: String,
    accounts: Vec<Account>,
    strava_url: String,
}

#[derive(Deserialize)]
struct RefreshResponse {
    refresh_token: String,
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
    #[allow(dead_code)]
    expires_in: u32,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Activity {
    id: i64,
    name: String,
    sport_type: String,
}

fn main() {
    let config = match load_env_variables() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Configuration error: {}", error);
            std::process::exit(1);
        }
    };

    if config.accounts.is_empty() {
        println!("No account configured. Authorize an account here:");
        println!("{}", build_auth_url(&config));
        return;
    }

    let after = unix_now().saturating_sub(LOOKBACK_SECONDS);
    let mut accounts = config.accounts.clone();

    for account in accounts.iter_mut() {
        match refresh_account_token(&config, account) {
            Ok(refreshed) => *account = refreshed,
            Err(error) => {
                eprintln!("{}: token refresh failed: {}", account.name, error);
                continue;
            }
        }

        if !account.force_ebike {
            continue;
        }

        if let Err(error) = convert_rides_to_ebike(&config, account, after) {
            eprintln!("{}: {}", account.name, error);
        }
    }

    if let Err(error) = write_accounts_to_env(Path::new(ENV_FILE), &accounts) {
        eprintln!("Failed to store rotated refresh tokens: {}", error);
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time is before the unix epoch")
        .as_secs()
}

fn load_env_variables() -> Result<StravaConfig, String> {
    #[cfg(not(test))] // Only load .env variables if we are not running tests
    {
        dotenv().ok(); // Load .env variables
    }

    let client_id: u32 = match env::var("STRAVA_CLIENT_ID") {
        Ok(value) => value
            .parse::<u32>()
            .map_err(|_| "Invalid STRAVA_CLIENT_ID".to_string())?,
        Err(_) => return Err("STRAVA_CLIENT_ID not set".to_string()),
    };

    let client_secret = match env::var("STRAVA_CLIENT_SECRET") {
        Ok(value) => value,
        Err(_) => return Err("STRAVA_CLIENT_SECRET not set".to_string()),
    };

    let accounts = load_accounts()?;

    let redirect_uri = match env::var("STRAVA_REDIRECT_URI") {
        Ok(value) => value,
        Err(_) => "http://localhost/".to_string(),
    };

    Ok(StravaConfig {
        client_id,
        client_secret,
        redirect_uri,
        accounts,
        strava_url: "https://www.strava.com".to_string(),
    })
}

/// Reads `STRAVA_ACCOUNTS`, or falls back to a single account built from the
/// legacy `STRAVA_REFRESH_TOKEN` key.
fn load_accounts() -> Result<Vec<Account>, String> {
    if let Ok(value) = env::var(ACCOUNTS_KEY) {
        let accounts: Vec<Account> = serde_json::from_str(&value)
            .map_err(|error| format!("Invalid {}: {}", ACCOUNTS_KEY, error))?;
        return Ok(accounts);
    }

    match env::var("STRAVA_REFRESH_TOKEN") {
        Ok(refresh_token) => Ok(vec![Account {
            name: "default".to_string(),
            refresh_token,
            force_ebike: false,
            access_token: None,
        }]),
        Err(_) => Ok(Vec::new()),
    }
}

fn build_auth_url(config: &StravaConfig) -> String {
    let encoded_redirect_uri = encode(&config.redirect_uri);
    format!("https://www.strava.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope=read,activity:read_all,activity:write", &config.client_id, encoded_redirect_uri)
}

fn refresh_account_token(config: &StravaConfig, account: &Account) -> Result<Account, String> {
    let url = Url::parse(format!("{}/oauth/token", config.strava_url).as_str())
        .map_err(|error| format!("Failed to parse Strava URL: {}", error))?;

    let data = [
        ("client_id", config.client_id.to_string()),
        ("client_secret", config.client_secret.clone()),
        ("refresh_token", account.refresh_token.clone()),
        ("grant_type", "refresh_token".to_string()),
    ];

    let response = Client::new()
        .post(url)
        .form(&data)
        .send()
        .map_err(|error| format!("Failed to send request: {}", error))?;

    if response.status() != 200 {
        return Err(format!("Failed to refresh token: {}", response.status()));
    }

    let body = response
        .text()
        .map_err(|error| format!("Failed to read response body: {}", error))?;
    let json: RefreshResponse =
        serde_json::from_str(&body).map_err(|error| format!("Failed to parse JSON: {}", error))?;

    Ok(Account {
        name: account.name.clone(),
        refresh_token: json.refresh_token,
        force_ebike: account.force_ebike,
        access_token: Some(json.access_token),
    })
}

/// Fetches the activities the account started after the given unix timestamp.
fn fetch_activities(
    config: &StravaConfig,
    account: &Account,
    after: u64,
) -> Result<Vec<Activity>, String> {
    let access_token = account
        .access_token
        .as_ref()
        .ok_or_else(|| "No access token available".to_string())?;

    let url = Url::parse(format!("{}/api/v3/athlete/activities", config.strava_url).as_str())
        .map_err(|error| format!("Failed to parse Strava URL: {}", error))?;

    let response = Client::new()
        .get(url)
        .bearer_auth(access_token)
        .query(&[
            ("after", after.to_string()),
            ("per_page", "100".to_string()),
        ])
        .send()
        .map_err(|error| format!("Failed to send request: {}", error))?;

    if response.status() != 200 {
        return Err(format!("Failed to fetch activities: {}", response.status()));
    }

    let body = response
        .text()
        .map_err(|error| format!("Failed to read response body: {}", error))?;

    serde_json::from_str(&body).map_err(|error| format!("Failed to parse JSON: {}", error))
}

/// True for activities that must become an e-bike ride.
fn is_convertible_ride(activity: &Activity) -> bool {
    activity.sport_type == RIDE_SPORT_TYPE
}

fn set_activity_sport_type(
    config: &StravaConfig,
    account: &Account,
    activity_id: i64,
    sport_type: &str,
) -> Result<(), String> {
    let access_token = account
        .access_token
        .as_ref()
        .ok_or_else(|| "No access token available".to_string())?;

    let url =
        Url::parse(format!("{}/api/v3/activities/{}", config.strava_url, activity_id).as_str())
            .map_err(|error| format!("Failed to parse Strava URL: {}", error))?;

    let response = Client::new()
        .put(url)
        .bearer_auth(access_token)
        .form(&[("sport_type", sport_type)])
        .send()
        .map_err(|error| format!("Failed to send request: {}", error))?;

    if response.status() != 200 {
        return Err(format!("Failed to update activity: {}", response.status()));
    }

    Ok(())
}

/// Converts every regular ride of the account started after `after`.
fn convert_rides_to_ebike(
    config: &StravaConfig,
    account: &Account,
    after: u64,
) -> Result<(), String> {
    let activities = fetch_activities(config, account, after)?;

    for activity in activities.iter().filter(|a| is_convertible_ride(a)) {
        match set_activity_sport_type(config, account, activity.id, EBIKE_SPORT_TYPE) {
            Ok(()) => println!(
                "{}: activity {} \"{}\" set to {}",
                account.name, activity.id, activity.name, EBIKE_SPORT_TYPE
            ),
            Err(error) => eprintln!(
                "{}: failed to update activity {}: {}",
                account.name, activity.id, error
            ),
        }
    }

    Ok(())
}

/// Writes the accounts back to the `.env` file, so rotated refresh tokens stay
/// usable on the next run. Does nothing if the file does not exist.
fn write_accounts_to_env(path: &Path, accounts: &[Account]) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let serialized = serde_json::to_string(accounts)
        .map_err(|error| format!("Failed to serialize accounts: {}", error))?;
    if serialized.contains('\'') || serialized.contains('\n') {
        return Err("Account data contains characters that break the .env format".to_string());
    }
    let new_line = format!("{}='{}'", ACCOUNTS_KEY, serialized);

    let content =
        fs::read_to_string(path).map_err(|error| format!("Failed to read .env: {}", error))?;

    let mut lines: Vec<String> = content.lines().map(|line| line.to_string()).collect();
    let accounts_prefix = format!("{}=", ACCOUNTS_KEY);
    // The legacy single token key is replaced by the account list.
    lines.retain(|line| !line.trim_start().starts_with("STRAVA_REFRESH_TOKEN="));

    match lines
        .iter()
        .position(|line| line.trim_start().starts_with(&accounts_prefix))
    {
        Some(index) => lines[index] = new_line,
        None => lines.push(new_line),
    }

    fs::write(path, format!("{}\n", lines.join("\n")))
        .map_err(|error| format!("Failed to write .env: {}", error))
}

#[cfg(test)]
mod load_env_variables_tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    fn set_valid_env_vars() {
        env::set_var("STRAVA_CLIENT_ID", "123456");
        env::set_var("STRAVA_CLIENT_SECRET", "dummy_secret");
        env::set_var(
            ACCOUNTS_KEY,
            r#"[{"name":"bene","refresh_token":"dummy_token","force_ebike":false},{"name":"wife","refresh_token":"wife_token","force_ebike":true}]"#,
        );
        env::remove_var("STRAVA_REFRESH_TOKEN");
    }

    fn expected_accounts() -> Vec<Account> {
        vec![
            Account {
                name: "bene".to_string(),
                refresh_token: "dummy_token".to_string(),
                force_ebike: false,
                access_token: None,
            },
            Account {
                name: "wife".to_string(),
                refresh_token: "wife_token".to_string(),
                force_ebike: true,
                access_token: None,
            },
        ]
    }

    #[test]
    #[serial(env)]
    fn test_load_env_variables_valid() {
        set_valid_env_vars();

        let expected = StravaConfig {
            client_id: 123456,
            client_secret: "dummy_secret".to_string(),
            redirect_uri: "http://localhost/".to_string(),
            accounts: expected_accounts(),
            strava_url: "https://www.strava.com".to_string(),
        };

        assert_eq!(load_env_variables().unwrap(), expected);
    }

    #[test]
    #[serial(env)]
    fn test_load_env_variables_invalid_client_id() {
        set_valid_env_vars();
        env::set_var("STRAVA_CLIENT_ID", "not_a_number");

        match load_env_variables() {
            Ok(_) => panic!("Expected an Err because STRAVA_CLIENT_ID is not a number"),
            Err(e) => assert_eq!(e, "Invalid STRAVA_CLIENT_ID"),
        }
    }

    #[test]
    #[serial(env)]
    fn test_load_env_variables_invalid_accounts() {
        set_valid_env_vars();
        env::set_var(ACCOUNTS_KEY, "not_json");

        match load_env_variables() {
            Ok(_) => panic!("Expected an Err because STRAVA_ACCOUNTS is not valid JSON"),
            Err(e) => assert!(e.starts_with("Invalid STRAVA_ACCOUNTS"), "got: {}", e),
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
    fn test_load_env_variables_missing_accounts() {
        set_valid_env_vars();
        env::remove_var(ACCOUNTS_KEY);

        assert_eq!(load_env_variables().unwrap().accounts, Vec::new());
    }

    #[test]
    #[serial(env)]
    fn test_load_env_variables_legacy_refresh_token() {
        set_valid_env_vars();
        env::remove_var(ACCOUNTS_KEY);
        env::set_var("STRAVA_REFRESH_TOKEN", "legacy_token");

        let expected = vec![Account {
            name: "default".to_string(),
            refresh_token: "legacy_token".to_string(),
            force_ebike: false,
            access_token: None,
        }];

        assert_eq!(load_env_variables().unwrap().accounts, expected);
    }
}

#[cfg(test)]
mod build_auth_url_tests {
    use super::*;

    #[test]
    fn test_build_auth_url() {
        let client_id: u32 = 123456;

        let config = StravaConfig {
            client_id,
            client_secret: "dummy_secret".to_string(),
            redirect_uri: "http://localhost/".to_string(),
            accounts: Vec::new(),
            strava_url: "https://www.strava.com".to_string(),
        };

        let expected_url = format!("https://www.strava.com/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope=read,activity:read_all,activity:write", client_id, "http%3A%2F%2Flocalhost%2F");
        let actual_url = build_auth_url(&config);
        assert_eq!(expected_url, actual_url);
    }
}

#[cfg(test)]
mod test_helpers {
    use super::*;

    pub(super) fn test_config(strava_url: String) -> StravaConfig {
        StravaConfig {
            client_id: 123456,
            client_secret: "dummy_secret".to_string(),
            redirect_uri: "http://localhost/".to_string(),
            accounts: Vec::new(),
            strava_url,
        }
    }

    pub(super) fn test_account(access_token: Option<String>) -> Account {
        Account {
            name: "wife".to_string(),
            refresh_token: "dummy_token".to_string(),
            force_ebike: true,
            access_token,
        }
    }
}

#[cfg(test)]
mod refresh_account_token_tests {
    use super::test_helpers::*;
    use super::*;
    use mockito::Matcher;

    #[test]
    fn test_refresh_account_token() {
        let mut server = mockito::Server::new();
        let config = test_config(server.url());
        let account = test_account(None);

        let expected = Account {
            name: "wife".to_string(),
            refresh_token: "new_refresh_token".to_string(),
            force_ebike: true,
            access_token: Some("dummy_access_token".to_string()),
        };

        let mock = server.mock("POST", "/oauth/token")
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body(
                Matcher::AllOf(vec![Matcher::UrlEncoded("client_id".to_string(), "123456".to_string()), Matcher::UrlEncoded("client_secret".to_string(), "dummy_secret".to_string()), Matcher::UrlEncoded("refresh_token".to_string(), "dummy_token".to_string()), Matcher::UrlEncoded("grant_type".to_string(), "refresh_token".to_string())])
            )
            .with_status(200)
            .with_body(r#"{"refresh_token":"new_refresh_token","access_token":"dummy_access_token","token_type":"Bearer","expires_in":21600}"#)
            .create();

        assert_eq!(refresh_account_token(&config, &account).unwrap(), expected);
        mock.assert();
    }

    #[test]
    fn test_refresh_account_token_error_status() {
        let mut server = mockito::Server::new();
        let config = test_config(server.url());
        let account = test_account(None);

        let mock = server
            .mock("POST", "/oauth/token")
            .with_status(401)
            .create();

        let error = refresh_account_token(&config, &account).unwrap_err();
        assert!(error.contains("401"), "got: {}", error);
        mock.assert();
    }
}

#[cfg(test)]
mod activity_tests {
    use super::test_helpers::*;
    use super::*;
    use mockito::Matcher;

    #[test]
    fn test_is_convertible_ride() {
        let cases = [
            ("Ride", true),
            ("EBikeRide", false),
            ("MountainBikeRide", false),
            ("VirtualRide", false),
            ("Run", false),
        ];

        for (sport_type, expected) in cases {
            let activity = Activity {
                id: 1,
                name: "test".to_string(),
                sport_type: sport_type.to_string(),
            };
            assert_eq!(is_convertible_ride(&activity), expected, "{}", sport_type);
        }
    }

    #[test]
    fn test_fetch_activities() {
        let mut server = mockito::Server::new();
        let config = test_config(server.url());
        let account = test_account(Some("dummy_access_token".to_string()));

        let mock = server
            .mock("GET", "/api/v3/athlete/activities")
            .match_header("authorization", "Bearer dummy_access_token")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("after".to_string(), "1700000000".to_string()),
                Matcher::UrlEncoded("per_page".to_string(), "100".to_string()),
            ]))
            .with_status(200)
            .with_body(
                r#"[{"id":1,"name":"Morning Ride","sport_type":"Ride"},{"id":2,"name":"Morning Run","sport_type":"Run"}]"#,
            )
            .create();

        let expected = vec![
            Activity {
                id: 1,
                name: "Morning Ride".to_string(),
                sport_type: "Ride".to_string(),
            },
            Activity {
                id: 2,
                name: "Morning Run".to_string(),
                sport_type: "Run".to_string(),
            },
        ];

        assert_eq!(
            fetch_activities(&config, &account, 1_700_000_000).unwrap(),
            expected
        );
        mock.assert();
    }

    #[test]
    fn test_fetch_activities_without_access_token() {
        let config = test_config("https://www.strava.com".to_string());
        let account = test_account(None);

        assert_eq!(
            fetch_activities(&config, &account, 0).unwrap_err(),
            "No access token available"
        );
    }

    #[test]
    fn test_set_activity_sport_type() {
        let mut server = mockito::Server::new();
        let config = test_config(server.url());
        let account = test_account(Some("dummy_access_token".to_string()));

        let mock = server
            .mock("PUT", "/api/v3/activities/42")
            .match_header("authorization", "Bearer dummy_access_token")
            .match_body(Matcher::UrlEncoded(
                "sport_type".to_string(),
                "EBikeRide".to_string(),
            ))
            .with_status(200)
            .with_body(r#"{"id":42,"sport_type":"EBikeRide"}"#)
            .create();

        assert_eq!(
            set_activity_sport_type(&config, &account, 42, EBIKE_SPORT_TYPE),
            Ok(())
        );
        mock.assert();
    }

    #[test]
    fn test_set_activity_sport_type_error_status() {
        let mut server = mockito::Server::new();
        let config = test_config(server.url());
        let account = test_account(Some("dummy_access_token".to_string()));

        let mock = server
            .mock("PUT", "/api/v3/activities/42")
            .with_status(403)
            .create();

        let error = set_activity_sport_type(&config, &account, 42, EBIKE_SPORT_TYPE).unwrap_err();
        assert!(error.contains("403"), "got: {}", error);
        mock.assert();
    }

    #[test]
    fn test_convert_rides_to_ebike_updates_only_rides() {
        let mut server = mockito::Server::new();
        let config = test_config(server.url());
        let account = test_account(Some("dummy_access_token".to_string()));

        let list_mock = server
            .mock("GET", "/api/v3/athlete/activities")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_body(
                r#"[{"id":1,"name":"Morning Ride","sport_type":"Ride"},{"id":2,"name":"Morning Run","sport_type":"Run"},{"id":3,"name":"Evening Ride","sport_type":"EBikeRide"}]"#,
            )
            .create();

        let update_mock = server
            .mock("PUT", "/api/v3/activities/1")
            .match_body(Matcher::UrlEncoded(
                "sport_type".to_string(),
                "EBikeRide".to_string(),
            ))
            .with_status(200)
            .expect(1)
            .create();

        assert_eq!(convert_rides_to_ebike(&config, &account, 0), Ok(()));
        list_mock.assert();
        update_mock.assert();
    }
}

#[cfg(test)]
mod write_accounts_to_env_tests {
    use super::*;

    fn accounts() -> Vec<Account> {
        vec![Account {
            name: "wife".to_string(),
            refresh_token: "new_token".to_string(),
            force_ebike: true,
            access_token: Some("secret_access_token".to_string()),
        }]
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut path = env::temp_dir();
        path.push(format!("mady-my-strava-{}-{}", name, std::process::id()));
        path
    }

    #[test]
    fn test_write_accounts_replaces_existing_key() {
        let path = temp_path("replace");
        fs::write(
            &path,
            "STRAVA_CLIENT_ID=123456\nSTRAVA_ACCOUNTS='[{\"name\":\"wife\",\"refresh_token\":\"old_token\",\"force_ebike\":true}]'\n",
        )
        .unwrap();

        write_accounts_to_env(&path, &accounts()).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        fs::remove_file(&path).unwrap();

        assert_eq!(
            content,
            "STRAVA_CLIENT_ID=123456\nSTRAVA_ACCOUNTS='[{\"name\":\"wife\",\"refresh_token\":\"new_token\",\"force_ebike\":true}]'\n"
        );
        // The short lived access token never lands in the file.
        assert!(!content.contains("secret_access_token"));
    }

    #[test]
    fn test_write_accounts_replaces_legacy_key_and_appends() {
        let path = temp_path("legacy");
        fs::write(
            &path,
            "STRAVA_CLIENT_ID=123456\nSTRAVA_REFRESH_TOKEN=old_token\n",
        )
        .unwrap();

        write_accounts_to_env(&path, &accounts()).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        fs::remove_file(&path).unwrap();

        assert_eq!(
            content,
            "STRAVA_CLIENT_ID=123456\nSTRAVA_ACCOUNTS='[{\"name\":\"wife\",\"refresh_token\":\"new_token\",\"force_ebike\":true}]'\n"
        );
    }

    #[test]
    fn test_write_accounts_without_env_file() {
        let path = temp_path("missing");
        assert!(!path.exists());
        assert_eq!(write_accounts_to_env(&path, &accounts()), Ok(()));
        assert!(!path.exists());
    }
}
