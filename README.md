# mady-my-strava

[![Coverage Status](https://coveralls.io/repos/github/BeneKenobi/mady-my-strava/badge.svg?branch=rust-conversion)](https://coveralls.io/github/BeneKenobi/mady-my-strava?branch=rust-conversion)

Automatically converts recent bike rides into e-bike rides for the accounts that
ask for it.

## Configuration

Copy the values into a `.env` file:

```
STRAVA_CLIENT_ID=<your API app id>
STRAVA_CLIENT_SECRET=<your API app secret>
STRAVA_REDIRECT_URI=http://localhost/
STRAVA_ACCOUNTS='[{"name":"bene","refresh_token":"...","force_ebike":false},{"name":"wife","refresh_token":"...","force_ebike":true}]'
```

Every account needs its own `refresh_token`, all of them from the same Strava API
application. Run the tool without `STRAVA_ACCOUNTS` to get the authorization URL,
open it while logged in as the account you want to add, and exchange the returned
code for a refresh token.

`force_ebike: true` means: every activity of that account with the sport type
`Ride` that started within the last 24 hours is changed to `EBikeRide`. Other
sport types, such as `MountainBikeRide` or `VirtualRide`, stay untouched.

Strava rotates the refresh token on every run, so the tool writes the updated
tokens back into `.env`. A legacy `STRAVA_REFRESH_TOKEN` entry is migrated into
`STRAVA_ACCOUNTS` on the first run.
