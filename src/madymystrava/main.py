import os
from datetime import datetime, timedelta
from typing import Any, Dict, List
from urllib.parse import urlencode

import requests
from dotenv import load_dotenv, set_key

load_dotenv()

def build_auth_url(client_id: str, redirect_uri: str) -> str:
    params = {
        'client_id': client_id,
        'redirect_uri': redirect_uri,
        'response_type': 'code',
        'scope': 'read,activity:read,activity:write'
    }
    return f"https://www.strava.com/oauth/authorize?{urlencode(params)}"

def get_strava_tokens(client_id: str, client_secret: str, authorization_code: str) -> Dict[str, Any]:
    payload = {
        'client_id': client_id,
        'client_secret': client_secret,
        'code': authorization_code,
        'grant_type': 'authorization_code'
    }
    response = requests.post('https://www.strava.com/oauth/token', data=payload)
    if response.status_code == 200:
        return response.json()
    else:
        print(f"Failed to exchange code for tokens: {response.json()}")
        return {}

def refresh_strava_token(client_id: str, client_secret: str, refresh_token: str) -> Dict[str, Any]:
    payload = {
        'client_id': client_id,
        'client_secret': client_secret,
        'refresh_token': refresh_token,
        'grant_type': 'refresh_token'
    }
    response = requests.post('https://www.strava.com/oauth/token', data=payload)
    if response.status_code == 200:
        return response.json()
    else:
        print(f"Failed to refresh token: {response.json()}")
        return {}

def get_yoga_activities(access_token: str, after: int) -> List[Dict[str, Any]]:
    url = "https://www.strava.com/api/v3/athlete/activities"
    headers = {"Authorization": f"Bearer {access_token}"}
    params = {"after": after, "per_page": 100}

    response = requests.get(url, headers=headers, params=params)

    if response.status_code == 200:
        activities = response.json()
        return [activity for activity in activities if activity['type'] == 'Yoga']
    else:
        print(f"Error: {response.json()}")
        return []

def update_activity_name(access_token: str, activity_id: int, new_name: str) -> None:
    url = f"https://www.strava.com/api/v3/activities/{activity_id}"
    headers = {"Authorization": f"Bearer {access_token}"}
    params = {"name": new_name}

    response = requests.put(url, headers=headers, params=params)

    if response.status_code == 200:
        print(f"Successfully updated activity {activity_id}")
    else:
        print(f"Error: {response.json()}")

if __name__ == "__main__":
    CLIENT_ID = os.getenv("STRAVA_CLIENT_ID")
    CLIENT_SECRET = os.getenv("STRAVA_CLIENT_SECRET")

    if CLIENT_ID is None or CLIENT_SECRET is None:
        print("Error: Missing STRAVA_CLIENT_ID or STRAVA_CLIENT_SECRET environment variables.")
        exit(1)

    REFRESH_TOKEN = os.getenv("STRAVA_REFRESH_TOKEN")
    REDIRECT_URI = "http://localhost"

    NEW_NAME = "#yogamitmady"
    
    if not REFRESH_TOKEN:
        print(f"Navigate to the following URL to get your authorization code: {build_auth_url(CLIENT_ID, REDIRECT_URI)}")
        authorization_code = input("Enter the authorization code: ")
        token_data = get_strava_tokens(CLIENT_ID, CLIENT_SECRET, authorization_code)
        REFRESH_TOKEN = token_data.get("refresh_token", "")
        if type(REFRESH_TOKEN) is not str:
            print("Error: Failed to get refresh token.")
            exit(1)
        set_key(".env", "STRAVA_REFRESH_TOKEN", REFRESH_TOKEN)

    token_data = refresh_strava_token(CLIENT_ID, CLIENT_SECRET, REFRESH_TOKEN)
    ACCESS_TOKEN = token_data.get("access_token", "")

    # Calculate the timestamp for 24 hours ago
    now = datetime.now()
    yesterday = now - timedelta(days=1)
    after_timestamp = int(yesterday.timestamp())

    yoga_activities = get_yoga_activities(ACCESS_TOKEN, after_timestamp)

    for activity in yoga_activities:
        if activity['name'] != NEW_NAME:
            update_activity_name(ACCESS_TOKEN, activity['id'], NEW_NAME)
