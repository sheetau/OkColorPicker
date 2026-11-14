# CSP companion protocol

`okcolor-csp` owns pairing URL parsing, framing and a blocking TCP session. Run it
on a background worker. It contains no UI, screen capture, persistent credentials
or color-model conversion. The picker must explicitly connect this layer to its
controls; the protocol crate alone does not enable application synchronization.

CSP's [Companion Mode](https://help.clip-studio.com/en-us/manual_en/840_options/Companion_Mode.htm)
exposes a QR through **Connect to smartphone**. The wire implementation references
[chocolatkey/clipremote](https://github.com/chocolatkey/clipremote) at
`582daf23132a892b46d75e24e9ec2e1e0caba0f0`, licensed under MIT. That implementation
documents a reverse-engineered protocol, not a supported CELSYS integration API.
Retain its [license](../licenses/clipremote-MIT.txt) when distributing this code.

## Pairing and authentication

`Pairing::parse` accepts an HTTPS URL on `companion.clip-studio.com` with a `/rc/`
path and exactly one `s` parameter. It decodes the hex/XOR payload into literal IP
addresses, a nonzero 16-bit port, a password and the server generation string.
Parsing neither resolves DNS nor contacts the URL host. The URL is limited to
4,096 bytes, addresses to eight distinct candidates, and credential fields to
bounded printable ASCII. Malformed data produces a generic error without secrets.

`Session::connect` tries the listed endpoints under one overall deadline, with a
750 ms ceiling for each address. Authentication sends the generation and the
obfuscated current/new passwords. A fresh eight-character reconnect password
comes from OS randomness. A `ReconnectToken` is exposed only after successful
authentication; reconnection sends the protocol marker and this rotated password.
The QR password is not reused as the reconnect credential.

Credentials live only in memory. `Pairing`, `ReconnectToken` and frame bodies are
redacted in debug formatting and are not serializable configuration types. Do not
log raw URLs, authentication frames or decoded secrets in callers.

## Frames and socket lifecycle

The leading byte is `0x01` for a command, `0x06` for success and `0x15` for failure.
Fields use the `0x1e $` delimiter and end with `0x1e 0x00`. JSON detail may be empty,
a scalar, an object or an array; a `0x0b` separator can introduce a binary tail.
The stream decoder retains partial frames and accepts multiple frames per read.
Each frame is limited to 1 MiB. A malformed or oversized incoming frame invalidates
the connection. The receive queue never relies on one read equaling one frame.

Outgoing serials start at zero per connection and never wrap. A reply must match
both the pending serial and command. Stale serials cannot satisfy another request.
Peer commands are acknowledged while waiting for replies and retained in a bounded
queue of 16 events. Authentication requires a success packet with a recognized
success body; a missing/malformed body or explicit rejection is not success.

Each operation has a deadline. A command timeout closes the socket, so a delayed
reply cannot be mistaken for a later request. An idle `poll` timeout instead keeps
partial bytes for the next poll. Explicit reconnect creates fresh receive queues
and serial state. The worker must send a heartbeat once per second while connected,
handle shutdown pushes, schedule reconnects and cancel on window/application exit.
It must also tag events with its own connection generation before applying them to
the model, independently of the server generation string used for authentication.

## Color boundary

`set_rgb` sends three 8-bit channels and `IsColorTransparent`. It omits
`ColorSpaceKind` and `ColorIndex`, whose presence would select a different command
variant. It does not send `SetAlpha`, which controls brush opacity.

Callers retain the original floating-point color/alpha and apply the chosen sRGB
output mapping only at this transport boundary. Received state is still raw JSON;
the integration must validate the particular state schema and channel ranges,
select the current main/sub color, and suppress echoes without quantizing the
editor's original color. Do not assume that every CSP integer color field is
8-bit RGB or that brush opacity is color alpha.

## Verification

Run `cargo test -p okcolor-csp --locked`. Tests cover a fixed pairing vector,
invalid fields, every split position of a frame, coalesced frames, bounded input,
real loopback sockets, interleaved pushes/ACKs, credential rotation/reconnect,
strict response matching, authentication rejection, partial reads across idle
timeouts and connection invalidation on request timeout.

Loopback peers establish these client contracts, not compatibility with a CSP
release. Release validation must also pair through a real CSP QR, inspect current
color states, send/receive colors, disconnect/reconnect and check echo suppression
while preserving the picker's floating-point source and history.
