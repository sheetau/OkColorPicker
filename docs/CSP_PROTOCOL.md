# CSP companion protocol

`okcolor-csp` owns pairing URL parsing, QR decoding, framing, a blocking TCP
session and its background worker. Native screen capture and the connection
control live in `okcolor-app`. Credentials remain in memory. After pairing, the
picker reads CSP's main/sub colors and active slot, and sends local color edits. This
integration is experimental and relies on a reverse-engineered protocol;
compatibility can vary between CSP versions.

CSP's [Companion Mode](https://help.clip-studio.com/en-us/manual_en/840_options/Companion_Mode.htm)
exposes a QR through **Connect to smartphone**. The wire implementation references
[chocolatkey/clipremote](https://github.com/chocolatkey/clipremote) at
`582daf23132a892b46d75e24e9ec2e1e0caba0f0`, licensed under MIT. That implementation
documents a reverse-engineered protocol, not a supported CELSYS integration API.
Retain its [license](../licenses/clipremote-MIT.txt) when distributing this code.

After a failed scan, **Paste pairing link** reads a copied Companion Mode URL
and runs the same pairing validation and authentication.

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

## Screen scanning and connection control

Open CSP's **Connect to smartphone** QR, keep the full code visible on one
display, then press **Connect CSP** in the picker. Press the status control again
to cancel or disconnect. Scanning runs once per explicit request. It neither
follows the QR URL in a browser nor saves screen images, URLs or credentials.
Unrelated and malformed QR codes are ignored; different valid CSP credentials
produce an ambiguity error instead of selecting the first host. Repeated copies
of the same credentials are accepted.

The Windows worker uses per-monitor-aware physical coordinates, including
negative desktop origins, and captures each active display through a top-down
32-bit GDI DIB. It restores the thread DPI context and releases all GDI resources
on success or failure. BGRA is reduced to greyscale before `rqrr` decoding;
unspecified bitmap alpha is ignored. Images are limited to 8,192 pixels per axis
and 16,777,216 pixels per display; at most 16 displays and 32 QR candidates are
accepted. Capture failure fails the whole scan so an unexamined display cannot
hide an ambiguous pairing. Occluded, minimized or split-across-display QR codes
must be made fully visible before scanning. Verify pairing with the CSP version
and desktop configuration targeted by each release.

The single worker coalesces pending requests, owns its socket and sleeps while
disconnected. Each request and each lost connection advance a checked local
generation; stale scan, authentication and push events cannot replace a later
request, cancellation or reconnected session.
The UI checks that generation again when applying asynchronous state. Initial
authentication has a two-second overall deadline. While connected, the worker
polls pushes with a 25 ms timeout and sends a heartbeat every second with a
500 ms response deadline. Heartbeats include `IdleTimerResetRequested: true`,
including immediately after authentication, to enable the server's state feed.
Unexpected disconnection or a server shutdown push
starts reconnection after one second. Failed attempts use one-second connection
deadlines and back off to two, then five seconds. Authentication rejection
requires a new scan. Disconnecting drops the in-memory reconnect token.

Closing the picker stops and joins the worker. Cancellation is checked between
display scans and after blocking operations; an in-progress QR decode or socket
deadline may finish before shutdown returns. Worker callbacks must remain short
and must not apply picker history directly from the background thread.

## Frames and socket lifecycle

The leading byte is `0x01` for a command, `0x06` for success and `0x15` for failure.
Fields use the `0x1e $` delimiter and end with `0x1e 0x00`. JSON detail may be empty,
a scalar, an object or an array; a `0x0b` separator can introduce a binary tail.
The stream decoder retains partial frames and accepts multiple frames per read.
Each frame is limited to 1 MiB. A malformed or oversized incoming frame invalidates
the connection. The receive queue never relies on one read equaling one frame.

Outgoing serials start at zero per connection and never wrap. A reply must match
both the pending serial and command. Stale serials cannot satisfy another request.
During authentication and low-level synchronous calls, peer commands are
acknowledged and retained in a bounded queue of 16 events. The connected worker
instead holds at most one color transaction and one independent heartbeat,
and returns each acknowledged host push to the UI before either completes. A slow
heartbeat or setter ACK cannot batch later color notifications behind it.
Authentication requires a success packet with a recognized success body; a missing/malformed body or explicit rejection is not success.

Each operation has a deadline. A command timeout closes the socket, so a delayed
reply cannot be mistaken for a later request. An idle `poll` timeout instead keeps
partial bytes for the next poll. Explicit reconnect creates fresh receive queues
and serial state. The worker must send a heartbeat once per second while connected,
handle shutdown pushes, schedule reconnects and cancel on window/application exit.
It must also tag events with its own connection generation before applying them to
the model, independently of the server generation string used for authentication.

## Color boundary

The low-level `set_rgb` sends three 8-bit channels and `IsColorTransparent`. It omits
`ColorSpaceKind` and `ColorIndex`, whose presence would select a different command
variant. It does not send `SetAlpha`, which controls brush opacity.

The worker's `ColorCommand` instead encodes bounded RGB as normalized unsigned
32-bit HSV and explicitly targets slot 0 (main) or 1 (sub). It carries the binary
transparent selector, never continuous alpha or brush opacity. This avoids a
queued edit silently targeting a different active slot. Commands explicitly select their target slot. A manipulating/disabled host or
an already acknowledged local revision is rejected before sending.

Received circle HSV/HLS and Sub View RGB are normalized over `u32::MAX`. This corrects
the older reference's 8-bit Sub View comment, using the captured-wire observations
in [LumaPaletteCSP's protocol notes](https://github.com/andyubird/LumaPaletteCSP/blob/9e63821f64f07e493f86273d0bfde78e1d4c829f/PROTOCOL.md).
Only protocol facts are used; the implementation is independent of that project's
GPL source. Field names, types and ranges are checked before an update is applied.
Incomplete state is merged with observations on the same socket; absent channels
never default to zero. Bad updates leave the cache unchanged and invalidate the
connection. Reconnection starts with an empty cache. Switching slots invalidates
the active RGB baseline, and switching models invalidates the channel baselines.
Both HSV and HLS circle payloads decode directly, including both slots and
partial updates during a gesture. HLS uses normalized H/L/S and converts to RGB
without another network read. The worker requests Sub View RGB only when the
circle color is incomplete or unknown, then rechecks circle metadata before
associating that fallback RGB with a slot.
If the slot changed during that read, the unconfirmed RGB is discarded and read
again. Tab-only responses cannot confirm RGB. Color Mix state does not stand in
for the active drawing color.

The worker polls circle state with an empty object every 25 ms (start to start),
reads Sub View as needed, and consumes pushes. It drains queued pushes in wire order before applying a later response.
Pending commands coalesce independently per main/sub slot and are sent in
revision order; connection replacement or loss clears both. A swap sends the
inactive slot first and the selected slot last, retaining the intended selection.
Setter acknowledgement produces `Sent`, and each observation reports the last
acknowledged local write revision. Slow replies never accumulate requests; color
reads and writes remain serialized
to preserve confirmation order. Heartbeat replies do not block polling. Recognized
color fields in other panel notifications use the same parser after the original
command has been acknowledged. A poll follows a successful write even if the
remote color is unchanged. This metadata lets the UI reject
observations predating a newer local edit. Transport errors do not replay writes
after reconnect. ACK alone does not establish that CSP applied the color.

Callers retain the original floating-point color/alpha and apply the chosen sRGB
output mapping only at this transport boundary. The application model integration
distinguishes local edits from received commits. Brush opacity is never
interpreted as color alpha.

## Picker synchronization

An initial connection adopts the observed CSP main/sub colors and selection. Received RGB is interpreted
as sRGB; the companion protocol exposes no document ICC profile. A different
stable remote color creates one Undo/recents commit and keeps the local alpha.
Neutral colors also retain the local editing hue. An ongoing CSP gesture updates
the picker live; release commits one Undo/recents entry for the whole gesture.
Local editing or disconnection commits the last received preview before proceeding.
Incoming colors wait while a text field has focus or a local drag is active;
the latest color is applied afterward. A committed local edit takes
precedence over an older deferred observation.

Local L/C/H edits, drag previews, cancellation and Undo/Redo send the selected
sRGB projection to the selected main/sub slot. Selecting a swatch selects that
slot in CSP; swapping or undoing a swap synchronizes both slots. A change to the output mapping also
updates that projection. Alpha-only edits are local. Sending a color selects
CSP's nontransparent drawing color; the picker alpha is not mapped to either
the transparent-color selector or brush opacity. The status reports CSP's
transparent selector separately.

The UI retains one bounded observation snapshot, tagged with the connection
generation and last acknowledged local write revision. Old-generation events
are discarded. Received commits carry a distinct origin and are never echoed.
An observation predating a pending local write cannot overwrite it. Confirmation
compares the projected RGB at CSP's possible 8-bit display precision; matching
observations leave every original f64 component, the editing hue, alpha and
history untouched. This also means sub-byte remote changes with the same rounded
RGB are not imported automatically.

If a send is rejected, remains unconfirmed for two seconds of observations, or
the connection is lost with a pending edit, the picker retains the local color.
Edits made while reconnecting likewise remain local. Once remote state is
available, **Use CSP color** explicitly adopts it, and **Send this color** submits
a fresh command. Neither an old unconfirmed command nor an offline edit is
automatically replayed on reconnection. A new local edit on a live connection can
also resolve a conflict. Explicitly starting a new pairing begins a fresh initial
read. Credentials, synchronization state and origin/revision metadata are never
written into the saved profile.

## Verification

Run `cargo test -p okcolor-csp --locked`. Tests cover a fixed pairing vector,
invalid fields, every split position of a frame, coalesced frames, bounded input,
real loopback sockets, interleaved pushes/ACKs, credential rotation/reconnect,
strict response matching, authentication rejection, partial reads across idle
timeouts and connection invalidation on request timeout. Color tests cover
unsigned channel endpoints, delta updates, model/slot changes, malformed state,
HSV/HLS conversion, the active heartbeat gate, coalesced writes, explicit slot
selection and observation order around setter acknowledgements. A loopback
fixture withholds a heartbeat ACK until multiple color pushes have reached the
UI-facing callback, establishing delivery independent of response completion.
This does not establish the notification cadence of a particular CSP installation;
interactive CSP follow-up still requires testing that installation.

Loopback peers establish these client contracts, not compatibility with a CSP
release. Release validation must also pair through a real CSP QR, inspect current
color states, send/receive colors, disconnect/reconnect and check echo suppression
while preserving the picker's floating-point source and history.
