import json
from typing import Any, Dict, List
from unittest.mock import Mock, patch

import pytest

from src.madymystrava.main import get_yoga_activities, update_activity_name

# Sample response for the /athlete/activities API
sample_activities_response: str = json.dumps([
    {
        "id": 1,
        "name": "Morning Run",
        "type": "Run"
    },
    {
        "id": 2,
        "name": "Evening Yoga",
        "type": "Yoga"
    },
    {
        "id": 3,
        "name": "#yogamitmady",
        "type": "Yoga"
    }
])

# Sample response for the /activities/{id} API
sample_update_response: str = json.dumps({
    "id": 2,
    "name": "#yogamitmady",
    "type": "Yoga"
})

@patch('os.getenv')
@patch('requests.get')
def test_get_yoga_activities(mock_get: Mock, mock_getenv: Mock) -> None:
    mock_getenv.return_value = "dummy_token"
    mock_get.return_value.status_code = 200
    mock_get.return_value.json.return_value = json.loads(sample_activities_response)

    activities: List[Dict[str, Any]] = get_yoga_activities("dummy_token", 12345)
    assert len(activities) == 2
    assert activities[0]['id'] == 2
    assert activities[1]['id'] == 3

@patch('requests.put')
def test_update_activity_name(mock_put: Mock) -> None:
    mock_put.return_value.status_code = 200
    mock_put.return_value.json.return_value = json.loads(sample_update_response)

    update_activity_name("dummy_token", 2, "#yogamitmady")
    mock_put.assert_called_once()