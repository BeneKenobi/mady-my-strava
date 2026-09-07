import json
import os
from datetime import datetime, timedelta
from typing import Any, Dict, List
from urllib.parse import urlencode

import requests
from dotenv import load_dotenv, set_key

load_dotenv()

# File the account list is read from and written back to.
ENV_FILE = ".env"
# Name of the .env key holding the account list.
ACCOUNTS_KEY = "STRAVA_ACCOUNTS"
# Name the yoga activities are renamed to.
YOGA_NAME = "#yogamitmady"
# Sport type of a regular, non electric bike ride.
BIKE_SPORT_TYPE = "Ride"
# Sport type the regular bike rides are changed to.
EBIKE_SPORT_TYPE = "EBikeRide"
# How far back yoga activities are looked at.
YOGA_LOOKBACK = timedelta(days=3)
# How far back bike rides are looked at.
BIKE_LOOKBACK = timedelta(days=1)
# Tasks an account gets when it comes from the legacy STRAVA_REFRESH_TOKEN key.
LEGACY_ACCOUNT: Dict[str, Any] = {
    "name": "default",
    "rename_yoga": True,
    "force_ebike": False,
}


def build_auth_url(client_id: str, redirect_uri: str) -> str:
    params = {
        "client_id": client_id,
        "redirect_uri": redirect_uri,
        "response_type": "code",
        "scope": "read,activity:read_all,activity:write",
    }
    return f"https://www.strava.com/oauth/authorize?{urlencode(params)}"


def get_strava_tokens(
    client_id: str, client_secret: str, authorization_code: str
) -> Dict[str, Any]:
    payload = {
        "client_id": client_id,
        "client_secret": client_secret,
        "code": authorization_code,
        "grant_type": "authorization_code",
    }
    response = requests.post("https://www.strava.com/oauth/token", data=payload)
    if response.status_code == 200:
        return response.json()
    else:
        print(f"Failed to exchange code for tokens: {response.json()}")
        return {}


def refresh_strava_token(
    client_id: str, client_secret: str, refresh_token: str
) -> Dict[str, Any]:
    payload = {
        "client_id": client_id,
        "client_secret": client_secret,
        "refresh_token": refresh_token,
        "grant_type": "refresh_token",
    }
    response = requests.post("https://www.strava.com/oauth/token", data=payload)
    if response.status_code == 200:
        return response.json()
    else:
        print(f"Failed to refresh token: {response.json()}")
        return {}


def load_accounts() -> List[Dict[str, Any]]:
    """Read the account list, or build one from the legacy single token key."""
    raw_accounts = os.getenv(ACCOUNTS_KEY)
    if raw_accounts:
        accounts: List[Dict[str, Any]] = json.loads(raw_accounts)
        return accounts

    refresh_token = os.getenv("STRAVA_REFRESH_TOKEN")
    if refresh_token:
        return [{**LEGACY_ACCOUNT, "refresh_token": refresh_token}]

    return []


def store_accounts(accounts: List[Dict[str, Any]]) -> None:
    set_key(ENV_FILE, ACCOUNTS_KEY, json.dumps(accounts))


def get_activities(access_token: str, after: int) -> List[Dict[str, Any]]:
    url = "https://www.strava.com/api/v3/athlete/activities"
    headers = {"Authorization": f"Bearer {access_token}"}
    params = {"after": after, "per_page": 100}

    response = requests.get(url, headers=headers, params=params)

    if response.status_code == 200:
        activities: List[Dict[str, Any]] = response.json()
        return activities
    else:
        print(f"Error: {response.json()}")
        return []


def get_yoga_activities(access_token: str, after: int) -> List[Dict[str, Any]]:
    activities = get_activities(access_token, after)
    return [activity for activity in activities if activity["type"] == "Yoga"]


def get_bike_activities(access_token: str, after: int) -> List[Dict[str, Any]]:
    """Return the regular bike rides, so neither e-bike nor mountain bike rides."""
    activities = get_activities(access_token, after)
    return [
        activity
        for activity in activities
        if activity.get("sport_type") == BIKE_SPORT_TYPE
    ]


def update_activity_name(access_token: str, activity_id: int, new_name: str) -> None:
    url = f"https://www.strava.com/api/v3/activities/{activity_id}"
    headers = {"Authorization": f"Bearer {access_token}"}
    params = {"name": new_name}

    response = requests.put(url, headers=headers, params=params)

    if response.status_code == 200:
        print(f"Successfully updated activity {activity_id}")
    else:
        print(f"Error: {response.json()}")


def update_activity_sport_type(
    access_token: str, activity_id: int, sport_type: str
) -> None:
    url = f"https://www.strava.com/api/v3/activities/{activity_id}"
    headers = {"Authorization": f"Bearer {access_token}"}

    # Strava answers 200 but keeps the old sport type when this is sent as a
    # query parameter or form encoded, so the value has to go in a JSON body.
    response = requests.put(url, headers=headers, json={"sport_type": sport_type})

    if response.status_code == 200:
        print(f"Successfully set activity {activity_id} to {sport_type}")
    else:
        print(f"Error: {response.json()}")


def rename_yoga_activities(access_token: str, after: int) -> None:
    for activity in get_yoga_activities(access_token, after):
        if activity["name"] != YOGA_NAME:
            update_activity_name(access_token, activity["id"], YOGA_NAME)


def convert_rides_to_ebike(access_token: str, after: int) -> None:
    for activity in get_bike_activities(access_token, after):
        update_activity_sport_type(access_token, activity["id"], EBIKE_SPORT_TYPE)


def process_account(
    account: Dict[str, Any], client_id: str, client_secret: str
) -> Dict[str, Any]:
    """Run the tasks the account asks for, return it with the rotated token.

    An account can carry its own client_id and client_secret. Strava caps how
    many athletes one API app connects, so a second athlete needs a second app.
    """
    token_data = refresh_strava_token(
        account.get("client_id", client_id),
        account.get("client_secret", client_secret),
        account["refresh_token"],
    )
    access_token = token_data.get("access_token", "")

    if not access_token:
        print(f"{account['name']}: no access token, skipping")
        return account

    now = datetime.now()

    if account.get("rename_yoga"):
        rename_yoga_activities(access_token, int((now - YOGA_LOOKBACK).timestamp()))

    if account.get("force_ebike"):
        convert_rides_to_ebike(access_token, int((now - BIKE_LOOKBACK).timestamp()))

    return {
        **account,
        "refresh_token": token_data.get("refresh_token", account["refresh_token"]),
    }


if __name__ == "__main__":
    CLIENT_ID = os.getenv("STRAVA_CLIENT_ID")
    CLIENT_SECRET = os.getenv("STRAVA_CLIENT_SECRET")

    if CLIENT_ID is None or CLIENT_SECRET is None:
        print(
            "Error: Missing STRAVA_CLIENT_ID or STRAVA_CLIENT_SECRET environment"
            " variables."
        )
        exit(1)

    REDIRECT_URI = "http://localhost"

    accounts = load_accounts()

    if not accounts:
        print(
            "Navigate to the following URL, logged in as the account you want to add,"
            f" to get your authorization code: {build_auth_url(CLIENT_ID, REDIRECT_URI)}"
        )
        authorization_code = input("Enter the authorization code: ")
        token_data = get_strava_tokens(CLIENT_ID, CLIENT_SECRET, authorization_code)
        new_refresh_token = token_data.get("refresh_token", "")
        if not isinstance(new_refresh_token, str) or not new_refresh_token:
            print("Error: Failed to get refresh token.")
            exit(1)
        accounts = [{**LEGACY_ACCOUNT, "refresh_token": new_refresh_token}]
        store_accounts(accounts)

    updated_accounts = [
        process_account(account, CLIENT_ID, CLIENT_SECRET) for account in accounts
    ]

    if updated_accounts != accounts:
        store_accounts(updated_accounts)
