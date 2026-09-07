import json
from typing import Any, Dict, List
from unittest.mock import Mock, patch

from src.madymystrava.main import (
    EBIKE_SPORT_TYPE,
    build_auth_url,
    get_bike_activities,
    get_yoga_activities,
    load_accounts,
    process_account,
    update_activity_name,
    update_activity_sport_type,
)

# Sample response for the /athlete/activities API
sample_activities_response: str = json.dumps(
    [
        {"id": 1, "name": "Morning Run", "type": "Run", "sport_type": "Run"},
        {"id": 2, "name": "Evening Yoga", "type": "Yoga", "sport_type": "Yoga"},
        {"id": 3, "name": "#yogamitmady", "type": "Yoga", "sport_type": "Yoga"},
        {"id": 4, "name": "Morning Ride", "type": "Ride", "sport_type": "Ride"},
        {"id": 5, "name": "Evening Ride", "type": "Ride", "sport_type": "EBikeRide"},
        {
            "id": 6,
            "name": "Trail Ride",
            "type": "Ride",
            "sport_type": "MountainBikeRide",
        },
    ]
)

# Sample response for the /activities/{id} API
sample_update_response: str = json.dumps(
    {"id": 2, "name": "#yogamitmady", "type": "Yoga"}
)


@patch("os.getenv")
@patch("requests.get")
def test_get_yoga_activities(mock_get: Mock, mock_getenv: Mock) -> None:
    mock_getenv.return_value = "dummy_token"
    mock_get.return_value.status_code = 200
    mock_get.return_value.json.return_value = json.loads(sample_activities_response)

    activities: List[Dict[str, Any]] = get_yoga_activities("dummy_token", 12345)
    assert len(activities) == 2
    assert activities[0]["id"] == 2
    assert activities[1]["id"] == 3


@patch("requests.put")
def test_update_activity_name(mock_put: Mock) -> None:
    mock_put.return_value.status_code = 200
    mock_put.return_value.json.return_value = json.loads(sample_update_response)

    update_activity_name("dummy_token", 2, "#yogamitmady")
    mock_put.assert_called_once()


def test_build_auth_url():
    client_id = "12345"
    redirect_uri = "https://example.com/callback"
    expected_url = (
        "https://www.strava.com/oauth/authorize?"
        f"client_id={client_id}"
        "&redirect_uri=https%3A%2F%2Fexample.com%2Fcallback"
        "&response_type=code"
        "&scope=read%2Cactivity%3Aread_all%2Cactivity%3Awrite"
    )
    assert build_auth_url(client_id, redirect_uri) == expected_url


# Sample response for the /activities/{id} API after a sport type change
sample_sport_type_update_response: str = json.dumps(
    {"id": 4, "name": "Morning Ride", "sport_type": "EBikeRide"}
)


@patch("requests.get")
def test_get_bike_activities(mock_get: Mock) -> None:
    mock_get.return_value.status_code = 200
    mock_get.return_value.json.return_value = json.loads(sample_activities_response)

    activities: List[Dict[str, Any]] = get_bike_activities("dummy_token", 12345)
    # Only the regular ride counts, not the e-bike and not the mountain bike ride.
    assert len(activities) == 1
    assert activities[0]["id"] == 4


@patch("requests.get")
def test_get_bike_activities_on_error(mock_get: Mock) -> None:
    mock_get.return_value.status_code = 401
    mock_get.return_value.json.return_value = {"message": "Authorization Error"}

    assert get_bike_activities("dummy_token", 12345) == []


@patch("requests.put")
def test_update_activity_sport_type(mock_put: Mock) -> None:
    mock_put.return_value.status_code = 200
    mock_put.return_value.json.return_value = json.loads(
        sample_sport_type_update_response
    )

    update_activity_sport_type("dummy_token", 4, EBIKE_SPORT_TYPE)

    # Strava only applies the sport type when it arrives as a JSON body.
    mock_put.assert_called_once_with(
        "https://www.strava.com/api/v3/activities/4",
        headers={"Authorization": "Bearer dummy_token"},
        json={"sport_type": "EBikeRide"},
    )


@patch.dict(
    "os.environ",
    {
        "STRAVA_ACCOUNTS": json.dumps(
            [
                {
                    "name": "bene",
                    "refresh_token": "bene_token",
                    "rename_yoga": True,
                    "force_ebike": False,
                },
                {
                    "name": "mady",
                    "refresh_token": "mady_token",
                    "rename_yoga": False,
                    "force_ebike": True,
                },
            ]
        )
    },
    clear=True,
)
def test_load_accounts() -> None:
    accounts: List[Dict[str, Any]] = load_accounts()

    assert [account["name"] for account in accounts] == ["bene", "mady"]
    assert accounts[1]["force_ebike"] is True


@patch.dict("os.environ", {"STRAVA_REFRESH_TOKEN": "legacy_token"}, clear=True)
def test_load_accounts_falls_back_to_single_token() -> None:
    assert load_accounts() == [
        {
            "name": "default",
            "rename_yoga": True,
            "force_ebike": False,
            "refresh_token": "legacy_token",
        }
    ]


@patch.dict("os.environ", {}, clear=True)
def test_load_accounts_without_configuration() -> None:
    assert load_accounts() == []


@patch("src.madymystrava.main.convert_rides_to_ebike")
@patch("src.madymystrava.main.rename_yoga_activities")
@patch("src.madymystrava.main.refresh_strava_token")
def test_process_account_runs_only_the_wanted_tasks(
    mock_refresh: Mock, mock_rename_yoga: Mock, mock_convert_rides: Mock
) -> None:
    mock_refresh.return_value = {
        "access_token": "dummy_access_token",
        "refresh_token": "rotated_token",
    }
    account = {
        "name": "mady",
        "refresh_token": "mady_token",
        "rename_yoga": False,
        "force_ebike": True,
    }

    updated_account = process_account(account, "12345", "dummy_secret")

    mock_rename_yoga.assert_not_called()
    mock_convert_rides.assert_called_once()
    assert mock_convert_rides.call_args[0][0] == "dummy_access_token"
    # The rotated refresh token is handed back, so it can be stored.
    assert updated_account["refresh_token"] == "rotated_token"


@patch("src.madymystrava.main.convert_rides_to_ebike")
@patch("src.madymystrava.main.rename_yoga_activities")
@patch("src.madymystrava.main.refresh_strava_token")
def test_process_account_without_access_token(
    mock_refresh: Mock, mock_rename_yoga: Mock, mock_convert_rides: Mock
) -> None:
    mock_refresh.return_value = {}
    account = {"name": "mady", "refresh_token": "mady_token", "force_ebike": True}

    assert process_account(account, "12345", "dummy_secret") == account
    mock_rename_yoga.assert_not_called()
    mock_convert_rides.assert_not_called()


@patch("src.madymystrava.main.convert_rides_to_ebike")
@patch("src.madymystrava.main.refresh_strava_token")
def test_process_account_uses_own_credentials(
    mock_refresh: Mock, mock_convert_rides: Mock
) -> None:
    mock_refresh.return_value = {
        "access_token": "dummy_access_token",
        "refresh_token": "mady_token",
    }
    account = {
        "name": "mady",
        "client_id": "654321",
        "client_secret": "mady_secret",
        "refresh_token": "mady_token",
        "force_ebike": True,
    }

    process_account(account, "12345", "dummy_secret")

    mock_refresh.assert_called_once_with("654321", "mady_secret", "mady_token")
    mock_convert_rides.assert_called_once()
