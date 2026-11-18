# HPE iLO 4 Integrated Remote Console protocol

What the .NET Integrated Remote Console (`IRC.exe`) speaks to an iLO. This
page was reconstructed from a decompile of that client (`IRC.decompiled.cs`).
Names in `code` are the client's own type and method names, so each claim can
be traced back to the decompile.

The core path has been checked against a live **iLO 4, firmware 2.82**: login,
`rc_info`, KVM and command-channel negotiation, both ciphers, a refresh
request, and a full decode of the video stream into a correct screen image.
[Verified against a live iLO 4](#verified-against-a-live-ilo-4) lists exactly
what was checked. Everything else is still from the decompile alone; see
[Open questions](#open-questions).

All multi-byte integers are **little-endian** unless a section says otherwise.
The exceptions are the SCSI CDBs and the replies in virtual media, which use
SCSI's big-endian layout.

## Overview

A console session uses up to four connections:

| Connection | Transport | Carries |
|---|---|---|
| Web API | HTTPS, port from the address (default 443) | Login, session info, `rc_info` (keys and ports), logout |
| KVM | TCP to `rc_port` | Video from the server (a bit-packed codec). Keyboard, mouse and a few control messages from the client |
| Command | TCP to `rc_port`, a second connection | Framed packets: power and health state, POST codes, seize and share prompts, virtual media replies, playback downloads, clipboard |
| Virtual media | TCP to `vm_port`, one connection per device | The iLO sends SCSI commands; the client answers from a local image or drive |

The KVM and command connections are not TLS. They are protected by a cipher
keyed from secrets delivered over HTTPS.

### Protocol versions

The client knows two generations, `Viewer.ProtocolVersion` 1 and 2
(`MaxProtocolVersion = 2`):

- **v1 (iLO 4).** Parameters come from `GET /json/rc_info`. Each TCP
  connection opens with a one-byte greeting and a session-key request. The
  streams use RC4 or AES-128 with static keys taken from `rc_info`.
- **v2 (newer firmware).** Parameters come from Redfish
  (`/redfish/v1/Managers/1/RcInfo/`). Each connection opens with an
  encrypted hello. Per-connection AES keys come from an HMAC-SHA512 KDF.

`GetParameters` tries v2 first and **falls back to v1 when the Redfish URL
returns 404**. That fallback is what happens on iLO 4: firmware 2.82 serves
`/redfish/v1/` but answers 404 for `Managers/1/RcInfo/`
(`Base.0.10.ResourceMissingAtURI`), and its `rc_info` reports
`"protocol_version": "1.1"`. The v1 path is therefore the one ServerRemote
needs, and v2 is documented for completeness.

## 1. Web API (HTTPS)

`Rest.send_request` behaviour:

- It accepts any server certificate. It allows TLS 1.0, 1.1 and 1.2.
- Every request sends the header `OData-Version: 4.0`. Once a session key is
  known, it also sends `X-Auth-Token: <session key>`.
- It sends back cookies the server set. `sessionKey` is the important one.
- Request bodies are `application/json`, UTF-8.

### Login

```
POST /json/login_session
{"method":"login", "user_login":"<user>", "password":"<password>"}
```

The response is JSON (`SessionInfo`):

| Field | Meaning |
|---|---|
| `session_key` | The session key. It is used everywhere below. On Integrity servers it is called `MPID` instead. |
| `user_name`, `user_account`, `user_dn`, `user_type`, `user_ip` | Informational |
| `user_expires` | Expiry time, format `ddd MMM dd HH:mm:ss yyyy` |
| `login_priv`, `remote_cons_priv`, `virtual_media_priv`, `reset_priv`, `config_priv`, `user_priv` | Privileges. iLO 4 sends them as integers `0`/`1`, not JSON booleans. The client hides virtual media without `virtual_media_priv` and hides power control without `reset_priv`. |

If the response sets a `sessionKey` cookie, the target is a ProLiant. If it
doesn't, the client treats it as an Integrity (BCS) iLO and sends the key as
an `MPID` cookie instead. A wrong password shows up as HTTP 403.

When the web UI launches the console, it passes the session key in the launch
URL instead (`?addr=&port=&sessionKey=` or `&MPID=`). The client then calls
`GET /json/session_info`, which returns the same `SessionInfo` shape without
logging in again.

### Logout

```
POST /json/login_session
{"method":"logout", "session_key":"<session key>"}
```

The client logs out on close only if it did the login itself, not when the
web UI launched it.

On iLO 4, logout **requires the `sessionKey` cookie**. Without the cookie
the server answers HTTP 500 with an empty body, and the session stays valid
(`session_info` still returns 200). With the cookie it answers 200, and the
session is gone (`session_info` returns 403).

A wrong password gets HTTP 403 with
`{"message":"JS_ERR_NO_PRIV","details":"Invalid login attempt"}`.

### v1 parameters: `GET /json/rc_info`

The response is a flat JSON object. `ApplyParameters_v1` doesn't use a JSON
parser. It strips `{}`, splits on `,` and then on `:`, and trims `"`. Values
therefore can't contain commas or colons. These are the keys it reads:

| Key | Use |
|---|---|
| `enc_key` | 32 hex characters, which give the 16-byte KVM key. The hex **string** is also kept: it is the XOR pad for the session key (see below). |
| `cmd_enc_key` | 32 hex characters, which give the 16-byte command-channel key |
| `rc_port` | TCP port for both the KVM and the command connection |
| `vm_port` | TCP port for virtual media (the client defaults to 17988) |
| `vm_key` | Parsed, but nothing in the client uses it |
| `https_port` | HTTPS port |
| `optional_features` | `;`-separated flags, listed below |
| `server_name`, `ilo_fqdn`, `blade`, `bay`, `enclosure` | Window title only. The string `"null"` means unset. |
| `enc_type`, `protocol_version` | Present in the `rc_info` contract but ignored by v1 parsing. Firmware 2.82 sent `enc_type: 0` and then chose RC4 in the session header, so the header, not `enc_type`, decides the cipher. |

A real `rc_info` from firmware 2.82, with the keys elided:

```json
{"enc_key":"<32 hex>","enc_type":0,"rc_port":17990,"vm_key":"<32 hex>",
 "vm_port":17988,"cmd_enc_key":"<32 hex>","protocol_version":"1.1",
 "optional_features":"ENCRYPT_KEY;ENCRYPT_VMKEY;ENCRYPT_CMD",
 "server_name":"","ilo_fqdn":"…","blade":0,"bay":null,"enclosure":null}
```

It has no `https_port`, and `bay` and `enclosure` are JSON `null` rather than
the string `"null"`. A real JSON parser handles both forms.

`enc_key` and `cmd_enc_key` are **new for every login** (8 logins gave 8
distinct pairs). `vm_key` was always equal to `enc_key`.

`optional_features` flags:

| Flag | Effect |
|---|---|
| `ENCRYPT_KEY` | Obfuscate the session key in the KVM and command negotiation |
| `ENCRYPT_VMKEY` | Obfuscate the session key in the virtual-media handshake, and set bit `0x4000` instead of `0x8000` in the negotiation |
| `ENCRYPT_CMD` | Encrypt the command channel |
| `REMOTE_CLIPBOARD_SUPPORT` | The iLO supports clipboard and file transfer over the command channel |
| `OFF_VIRTUAL_MEDIA` | Hide virtual media |
| `OFF_ILO_PLAYBACK` | No server-side capture buffers. Only local playback is offered. |

### v2 parameters

`GET /redfish/v1/Managers/1/RcInfo/` returns `RcInfo`:

| Field | Meaning |
|---|---|
| `Enabled` | bool. False means the remote console is disabled. |
| `MasterKey` | hex, the KDF master key |
| `ProtocolVersion` | int. A value above 2 means the client must be upgraded. |
| `RcPort` | u16 |
| `VmPort` | u16 |

For the window title, the client also sends `POST /redfish/v1/Views/` with a
`Select` body that asks for the NetworkService `VirtualMedia.ProtocolEnabled`,
`FQDN` and `HostName`, the Chassis `Oem.Hpe.BayNumber`, the Systems
`HostName` and `Model`, and the root
`Oem.Hpe.Manager.[0].Blade.EnclosureName`.

## 2. Connection setup

### v1 negotiation (KVM and command)

Everything in the negotiation is **plaintext**.

1. Open TCP to `rc_port`. The client sets `TCP_NODELAY` and a 30 s maximum
   retransmit time.
2. The server sends one byte, `0x50` (`'P'`). The KVM path aborts on any other
   byte. The command path skips up to 30 bytes looking for it.
3. The client sends `NEG_REQ_KEY` (34 bytes, packed):

   | Offset | Size | Field |
   |---|---|---|
   | 0 | u16 | `cmd`: `0x2001` for KVM, `0x2002` for the command channel |
   | 2 | 32 | Session key as ASCII, zero-padded |

   On Integrity iLOs the key field is 56 bytes (`NEG_REQ_KEY_BCS`).

   With `ENCRYPT_KEY`, each key byte `i` is XORed with
   `enc_key_str[i % len]`. That is the ASCII **hex text** of `enc_key`, not
   the decoded bytes. `cmd` is also ORed with `0x8000`, or with `0x4000` when
   `ENCRYPT_VMKEY` is also set.
4. The server replies with one status byte:

   | Byte | Meaning | Client action |
   |---|---|---|
   | `0x52` `'R'` | Granted, remote console licensed | Done |
   | `0x57` `'W'` | Granted, not licensed | Done (features limited) |
   | `0x53` `'S'` | Busy: someone else holds the console | Ask the user to seize or share |
   | `0x59` `'Y'` | Busy, and multi-user console isn't licensed | As `0x53` (share is pointless) |
   | `0x51` `'Q'` | Denied: session limit reached | Fail |
   | `0x58` `'X'` | No free sessions | Fail |
   | other | Treated as success | |

5. If the console is busy, the client sends a u16 with no key:
   - **Seize**: `0x0055` (`'U'`). Reply `0x52` granted, `0x51` denied.
   - **Share**: `0x0056` (`'V'`). Reply `0x52` granted, then
     [shared session](#shared-sessions); `0x51` denied.

The KVM connection is negotiated first. Once it succeeds, `connectCmd_v1`
opens the command connection and repeats the steps with `0x2002`.

Observed on firmware 2.82:

- When another session holds the console, the iLO answers `'S'`. It then
  waits for a seize or share request without closing the connection.
- An invalid session key gets `'Q'`, and the server closes the connection
  straight away.

### v2 hello (every channel)

The client builds a fresh `Connection` per channel. It takes the next 16
bytes of the [KDF](#v2-key-derivation) as the **client→server key**, then the
16 bytes after that as the **server→client key**. The KDF is one stream shared
by the whole session, so the KVM, command and each virtual-media connection
each consume the next 32 bytes, in the order they connect.

`ClientHello` (54 bytes, packed):

| Offset | Size | Field |
|---|---|---|
| 0 | 16 | IV (random). Sent in clear. |
| 16 | u8 | Command: 0 New, 1 Acquire (seize), 2 Share |
| 17 | u8 | Channel: 0 KVM, 1 Cmd, 2 Floppy, 3 Disc, 4 USB key, 5 Virtual folder |
| 18 | u32 | Flags (0) |
| 22 | 32 | Session key, ASCII |

Bytes 16 to 53 are encrypted with AES-OFB, using the client→server key and
that IV.

`ServerHello` (53 bytes): a 16-byte IV in clear, then, encrypted with the
server→client key and that IV:

| Offset | Size | Field |
|---|---|---|
| 16 | u8 | Status: 0 Success, 1 Denied, 2 Busy, 3 NotSupported, 4 BadRequest, 5 InternalError, 6 NoSessions, 7 NotLicensed |
| 17 | u32 | Flags |
| 21 | 32 | Session key |

Both keystreams carry on from the hello for the rest of the connection. On
`Busy`, the client reconnects with Command = Acquire or Share. A refused TCP
connection is reported as `Denied`.

## 3. Ciphers

### AES "OFB"

`Aes.TransformBytes` runs .NET AES in CBC mode with no padding, encrypts a
block of zeros over and over, and XORs the output into the data. CBC chains
each output into the next input, so the keystream is
`E(IV), E(E(IV)), …`, which is exactly **AES-OFB**. Encrypting and decrypting
are the same operation. Each direction keeps its own keystream, which runs
across all bytes sent or received.

- v1 KVM: key = `enc_key`, IV = 16 zero bytes.
- v1 command channel (with `ENCRYPT_CMD`): key = `cmd_enc_key`, IV = zeros.
- v2: per-connection keys from the KDF, IVs from the hellos.

### RC4

`Rc4` is standard RC4 with the 16-byte `enc_key`. It keeps a separate
instance for each direction.

### v1 KVM cipher selection

The KVM decoder starts with **no cipher**. The server picks one in the
[session header](#firmware-commands) (and with firmware command 12):

| Value | Cipher |
|---|---|
| 0 | none |
| 1 | RC4 |
| 2 | AES-128 |
| 3 | "AES-256", still keyed with the 16-byte `enc_key`, so in practice AES-128 |

Selecting a cipher **re-creates both keystreams from scratch**
(`InitCrypto`). The client sends nothing on the KVM connection until the
header has arrived (`KvmReadyToWrite`), so every client→server KVM byte is
encrypted from the start of the new keystream.

**Observed on firmware 2.82:**

1. The header is the first **7 plaintext bytes** (`1d 20 60 f0 90 61 b5`),
   sent as a TCP segment of its own. It decodes to one firmware command 13
   with params `[0, 1, 7, 9]`: 5-bit colour, RC4, licence bits 0 to 2 set,
   flags `0b1001`.
2. The header ends partway through its last byte. The remaining bits of that
   byte are padding.
3. From the **next byte (offset 7)** onwards the stream is encrypted with a
   fresh keystream. Its first decrypted bytes are
   `00 00 00 00 00 80 …`: more than 30 zero bits, so a
   [resync](#bit-reader) that discards the `0x80` and resets the decoder.

A decoder should therefore switch cipher at the byte boundary after the
header. It may misread the header's padding bits (the client briefly enters
`LATCHED` there), and the resync that follows puts it back in step.

### v1 command channel encryption

With `ENCRYPT_CMD`, the client sends the plaintext command `08 00 00 00`
immediately after negotiation. After that, both directions are AES-OFB with
`cmd_enc_key` and a zero IV.

On firmware 2.82 the channel also works if the client never sends `08`: the
iLO then sends its packets in plaintext, even though `rc_info` lists
`ENCRYPT_CMD`.

### v2 key derivation

`KeyDerivationFunction` is HMAC-SHA512 in feedback mode, keyed with
`MasterKey`. Block *i* (starting at 1) is:

```
K(i) = HMAC-SHA512(master,
         [K(i-1) if i > 1] || u32le(i) || "iLO IRC" || 0x00 || "key derivation" || u32le(L_i))
```

`L_1 = 512`, and L increases by 512 for every block (`_count += 512`). Keys
are drawn from the concatenated `K(1) K(2) …` stream, 64 bytes per block.

## 4. KVM: client → server

Every message starts with a u16 type. The bytes are encrypted with the KVM
cipher as described above.

| Type | Length | Layout | Meaning |
|---|---|---|---|
| `0x0000` | 4 | `00 00 op 00`. op: 0 momentary press, 1 press and hold, 2 power cycle, 3 reset | Virtual power button (`SendPower`) |
| `0x0001` | 10 | `01 00 mods 00 k1 k2 k3 k4 k5 k6` | Keyboard: a USB HID boot-keyboard report |
| `0x0002` | 10 | `02 00 xL xH yL yH dx dy buttons wheel` | Mouse |
| `0x0003` | 10 | `03 00` + 8 zero bytes | Shared viewer → leader: closing |
| `0x0004` | 10 | `04 00` + 8 zero bytes | Shared viewer → leader: opened |
| `0x0005` | 2 | `05 00` | Request a full refresh |
| `0x0007` | 3 | `07 00 mode` | Set colour depth. mode 0 to 7 = 15, 12, 9, 6, 5, 4, 3, 2 bpp (`cmd_bpp`) |
| `0x0008` | 2 | `08 00` | Request the "startup" capture buffer (playback) |
| `0x0009` | 2 | `09 00` | Request the "pre-failure" capture buffer |
| `0x000B` | 2 | `0B 00` | Closing the console |
| `0x000C` | 2 | `0C 00` | Acknowledge (answers firmware command 16) |
| `0x000D` | 10 / 20 | IPv4: `0D 00` + 4-byte address + 4 zero bytes. IPv6: `11 00 18 00` + 16-byte address | Leader → iLO: a shared viewer left |
| `0x000E` | 10 / 20 | IPv4: `0E 00` + address. IPv6: `10 00 18 00` + address | Leader → iLO: a shared viewer joined |
| `0x000F` | 2 | `0F 00` | Idle. While playing back locally, a single `0F` byte goes out every 240 s as a keepalive. |

**Keyboard.** `mods` is the HID modifier bitmap (bit 0 LCtrl … bit 7 RGUI).
`k1..k6` are HID usage codes for the keys held down, in no particular order.
The report is the full current state, not a delta. The client sends a report
whenever a key's state changes, and an all-zero report (`AllKeysUp`) on focus
loss and after the session header. Ctrl+Alt+Del is
`mods=0x05 (LCtrl|LAlt), k1=0x4C`. After 250 ms the client sends `mods=0x05`
with no keys, then the all-up report. Windows virtual keys are translated to
HID through a US layout table (`vk409_to_hid`); a Japanese 106/109 variant
also exists. The iLO therefore expects layout-independent HID usages.

**Mouse.**

- `x` and `y` are absolute positions scaled to **0 to 3000** across the
  current video mode: `3000 * px / width`, as a little-endian u16.
- `dx` and `dy` are relative motion clamped to ±127, encoded
  **sign-magnitude**: bit 7 is the sign and the low 7 bits are the
  magnitude. `dy` is positive **upwards** (the code computes
  `old_y - new_y`).
- `buttons`: bit 0 left, bit 1 right, bit 2 middle, bit 3 X1, bit 4 X2.
- `wheel` is the low byte of the WPF wheel delta (±120 per notch, so `0x78`
  or `0x88`).
- The client sends a report on every move and on every button change.

## 5. KVM: server → client video

The server→client KVM stream is a **bit-oriented codec**, which the client
calls "DVC". It is decoded by a state machine (`Viewer.StateMachine`, with the
state table built in `InitRemCons`). The screen is divided into **16×16
blocks**. In "half-height" mode (see `SetHalfHeight`) they are 16 wide by 8
tall.

### Bit reader

`GetBit(n)` reads n bits (0 ≤ n ≤ 8):

- Bytes are consumed in stream order. Within each byte, bits are taken
  **least-significant bit first**.
- An n-bit field is assembled with the **first bit taken as its most
  significant bit**.

**Resync.** The reader counts consecutive zero bits across byte boundaries
(using lookup tables of leading and trailing zeros). When a new byte pushes
the count above **30**, the reader treats it as a sync:

1. It discards zero bytes until it finds a non-zero byte, and discards that
   byte too.
2. It clears the bit accumulator.
3. It forces the state machine to `RESET`.

Encoded data therefore never contains 31 zero bits in a row. A run of zero
bytes followed by a non-zero byte acts as a frame and recovery marker.

### State machine

Each step:

1. Read `bits` bits from the current state, giving `code`.
2. Choose the next state: `state0` if `code == 0`, otherwise `state1`.
3. Run the state's action, which may override the next state.

The next state is chosen from the bits as read, before the action changes
`code`.

| # | Name | Bits | → if 0 | → if ≠0 | Action |
|---|---|---|---|---|---|
| 0 | RESET | 0 | 1 | 1 | Reset the colour cache, x = y = 0, pixel count = 0 |
| 1 | START | 1 | 2 PIXELS | 15 CMD | |
| 2 | PIXELS | 1 | 31 PIXFAN | 3 PIXLRU1 | |
| 3 | PIXLRU1 | 1 | 2 | 11 | Emit the cache colour with LRU rank 1. The bit read means "repeat follows". |
| 4 | PIXLRU0 | 1 | 2 | 11 | Emit the cache colour with LRU rank 0 |
| 5 | PIXCODE1 | 1 | 10 | 10 | Emit by rank: code 0 → rank 0, c → rank c+1 |
| 6 | PIXCODE2 | 2 | 10 | 10 | Same as 5 |
| 7 | PIXCODE3 | 3 | 10 | 10 | Same as 5 |
| 32 | PIXCODE4 | 4 | 10 | 10 | Same as 5 |
| 31 | PIXFAN | 1 | 36 PIXSPEC | *dynamic* | The ≠0 branch goes to the state for the current cache size (below) |
| 35 | PIXCODE | 0 | *dynamic* | | Unused jump to the cache-size state |
| 36 | PIXSPEC | 1 | 8 PIXGREY | 9 PIXRGBR | Explicit colour follows |
| 8 | PIXGREY | bpc | 10 | 10 | r = g = b = code. Add to the cache and emit. |
| 9 | PIXRGBR | bpc | 41 | 41 | r = code |
| 41 | PIXRGBG | bpc | 42 | 42 | g = code |
| 42 | PIXRGBB | bpc | 10 | 10 | b = code. Add `rgb` to the cache and emit. |
| 10 | PIXRPT | 1 | 2 | 11 | Is a repeat next? |
| 11 | PIXRPT1 | 1 | 33 PIXDUP | 12 | |
| 33 | PIXDUP | 0 | 2 | 2 | Emit the last colour once more |
| 12 | PIXRPTSTD1 | 3 | 2 | 2 | 0 to 5: emit the last colour code+2 times (2 to 7). 6 → state 13. 7 → state 14. |
| 13 | PIXRPTSTD2 | 3 | 2 | 2 | Emit the last colour code+8 times (8 to 15) |
| 14 | PIXRPTNSTD | 8 | 2 | 2 | Emit the last colour code times (expected ≥ 16) |
| 15 | CMD | 1 | 16 CMD0 | 17 MOVEXY0 | |
| 17 | MOVEXY0 | 7 (8) | 39 | 39 | newx = code |
| 39 | MOVEXY1 | 7 (8) | 1 | 1 | x = newx, y = code (masked to 7 bits in 16-row mode) |
| 16 | CMD0 | 1 | 19 CMDX | 18 EXTCMD | |
| 19 | CMDX | 1 | 20 | 21 | |
| 20 | MOVESHORTX | 3 | 1 | 1 | x = (x + code + 1) & 0x7F |
| 21 | MOVELONGX | 7 (8) | 1 | 1 | x = code |
| 18 | EXTCMD | 1 | 22 BLKRPT | 23 EXTCMD1 | |
| 22 | BLKRPT | 1 | 34 | 28 | |
| 34 | BLKDUP | 0 | 1 | 1 | Write the previous block again at (x, y) and advance x |
| 28 | BLKRPT1 | 1 | 29 | 30 | |
| 29 | BLKRPTSTD | 3 | 1 | 1 | Repeat the previous block code+2 times |
| 30 | BLKRPTNSTD | 7 (8) | 1 | 1 | Repeat the previous block code times |
| 23 | EXTCMD1 | 1 | 25 EXTCMD2 | 24 FIRMWARE | |
| 25 | EXTCMD2 | 1 | 26 MODE0 | 27 TIMEOUT | |
| 26 | MODE0 | 7 | 40 | 40 | newx = code |
| 40 | MODE1 | 7 | 47 | 47 | If the size changed, clear the screen. Width = newx blocks, height = code rows of 16. |
| 47 | MODE2 | 4 | 1 | 1 | extra = code. Set the mode to `width*16` × `height*16 + extra`. Reset position and cache. |
| 27 | TIMEOUT | 0 | 1 | 1 | Discard bits up to the next byte boundary. Two in a row → LATCHED. |
| 24 | FIRMWARE | 8 | 46 | 46 | One byte of a firmware command |
| 46 | CORP | 1 | 1 | 24 | 1 = another byte follows. 0 = run the command. |
| 44 | PRINT0 | 8 | 45 | 45 | Message kind |
| 45 | PRINT1 | 8 | 45 | 45 | One character. 0 ends the message (→ START). |
| 37 | EXIT | 0 | 37 | 37 | Stop the session |
| 38 | LATCHED | 1 | 38 | 38 | Error state. Wait for a resync. After 32768 latched bits, send a refresh (`05 00`). |
| 43 | HUNT | 1 | 43 | 0 | Defined, never entered |

Bit counts marked "7 (8)" become 8 in half-height mode, where block
coordinates can exceed 127.

**Blocks.** Pixels fill the current block in row-major order. When the
`PIXELS` state is reached with a full block (16 × block height pixels):

1. The block is written at block (x, y) and x advances.
2. The cache is pruned.
3. The machine goes back to `START`.

The block buffer keeps its contents afterwards, so `BLKDUP` and `BLKRPT*`
copy the last decoded block to later positions. Block repeats don't wrap to
the next row: positions past the mode width are skipped. Movement (`MOVE*`)
is in block units.

**Colours.** With `bpc` bits per channel (5, 4, 3 or 2), a colour is
`r << 2·bpc | g << bpc | b`. At 5 bits this is RGB555, with blue in the low
bits. To display it, scale each channel to 8 bits (`<< (8 − bpc)`).

The client's lookup table (`BuildPixelTable`) is correct for 5 and 4 bits.
For 3 and 2 bits it uses masks that don't line up with the fields, which
looks like a bug in the client rather than part of the protocol.

`bpc` is `5 − (value & 3)`, where value comes from the header or from
firmware command 11.

**Colour cache.** `CCache` holds up to 17 recent colours, each with an LRU
rank (0 = most recent):

- **Explicit colour** (states 8 and 42). The colour is inserted at rank 0 and
  the other ranks shift up. If the cache is full, the colour replaces the
  entry with the oldest rank. If the colour is *already* in the cache, that
  is a protocol error and the machine goes to `LATCHED`.
- **Colour by rank** (`find`). The entry moves to rank 0 and the entries
  ahead of it shift down. An unknown rank goes to `LATCHED`. With exactly one
  colour in the cache, every rank lookup returns that colour.
- **Ageing.** Each entry has a block counter. Using an entry sets its counter
  to 1. At each block boundary, `prune` removes entries whose counter is 0 and
  decrements the rest. In effect, a colour unused for a whole block is
  forgotten.
- **PIXFAN target.** The ≠0 branch of `PIXFAN` depends on the cache size
  after each insert or prune:

  | Cache size | Next state |
  |---|---|
  | 0 or 1 | 38 LATCHED |
  | 2 | 4 PIXLRU0 |
  | 3 | 5 PIXCODE1 |
  | 4 or 5 | 6 PIXCODE2 |
  | 6 to 9 | 7 PIXCODE3 |
  | ≥ 10 | 32 PIXCODE4 |

  Rank 1 has its own short path (`PIXELS` → 1 → `PIXLRU1`). The
  `PIXCODEn` states therefore count ranks as 0, 2, 3, …

### Firmware commands

A firmware command is a run of bytes, each read by `FIRMWARE` and followed
by one `CORP` bit (1 = more bytes). The **last** byte is the opcode, and the
bytes before it are its parameters, in order.

| Opcode | Params | Meaning |
|---|---|---|
| 1 | — | Exit (end the session) |
| 2 | — | Print. Then `PRINT0` reads a kind byte and `PRINT1` reads a NUL-terminated string. Kinds 1 to 3 go to the status bar, kind 4 to a message box. |
| 3 | rate | Frame rate (informational) |
| 4 | — | Server power on |
| 5 | — | Server power off. The client blanks the screen. |
| 6 | — | Clear the screen |
| 7, 8 | — | Ignored |
| 9 | — | Discard bits up to the next byte boundary |
| 10 | — | "Seize" notification (logged only) |
| 11 | bpc | Set bits per colour |
| 12 | enc | Set the cipher ([table](#v1-kvm-cipher-selection)) and restart the keystreams |
| 13 | bpc, enc, license, flags | **Session header**, sent at the start. The client may send only after it arrives. |
| 16 | — | Server asks for an ack. Reply `0C 00`. |
| 128 | — | End of frame: present the framebuffer |

Session header fields:

- **license** bits: 0 = remote console licensed, 1 = playback licensed,
  2 = multi-user console licensed, 3 = scripted virtual media *not* licensed.
- **flags** bits: 1 = URL floppy connected, 2 = URL DVD connected,
  3 = half-height (8-row blocks) supported.

Half-height mode is used only when the mode is wider than 1616 px and flag
bit 3 is set. The client caps the framebuffer at 2560×1600. A mode of 0×0
means "no video".

## 6. Command channel

### Server → client packets

Each packet is a 12-byte header followed by `size` bytes of data:

| Offset | Size | Field |
|---|---|---|
| 0 | u32 | `cmd` |
| 4 | u32 | `size` (the client rejects anything over 1500 and drains the socket) |
| 8 | u16 | `seq` |
| 10 | u16 | `flags` |

| cmd | Meaning | Data |
|---|---|---|
| 2 | Ignored | |
| 3 | Power state | `data[0]`: 0 off, 1 on |
| 4 | Health LED | `data[0]`: 0 normal, 1 caution, 2 critical, other inactive |
| 5 | POST code | u16, `data[1]` high byte and `data[0]` low byte |
| 6 | Another user wants to **seize** | `data[0..64]` user name, `data[64..128]` address, both NUL-padded ASCII. `flags` = timeout in seconds. Answer yes or no. The default is yes, after which this client closes. |
| 7 | Virtual-media URL ack | `data[0]` i8 result (−1 can't parse, −2/−5 already connected, −3 image not found, −4 config error, −6 not licensed, −7 can't connect). `data[1]` device group, `data[2]` device type (8 = URL), `data[3]` connected bool. |
| 8 | Playback capture download chunk | See [Playback](#playback-and-capture-files) |
| 9 | Another user asks to **share** | Same layout as cmd 6. The default answer is no. |
| 10 | iLO firmware update starting | The client closes |
| 11 | Unauthorized | `flags`: 2 remote console (stop), 3 virtual media (disconnect all), 4 power, other (stop) |
| 12 | Clipboard / file transfer | SMIF packet, see [below](#clipboard-and-file-transfer) |
| 13 | Virtual media switch | `data[0]` code (logged only) |
| 14 | Remote console not licensed | The client closes |
| 15 | iLO is resetting | The client closes |
| 16 | Clipboard helper status | `SMIF_0090` (`os_type`, `protocol_ver`, `action`, `reserved`, all u16). `action == 1` means the host-side helper is running. |
| 17 | SMIF send failed | `flags` = clipboard type that failed (1 text, 7 file data, 14 image) |

On firmware 2.82 the iLO sends a cmd 3 / cmd 4 pair (power state, then
health) when the channel opens, and **again about every 5 s** for as long as
it stays open. After the `08` encryption switch it also sent one cmd 16 with
eight zero bytes (`action` 0: the helper isn't running). All packets had
`seq` and `flags` set to 0.

### Client → server commands

Each command is four bytes, `[code, 0, 0, 0]`, sometimes followed by a
payload:

| Code | Payload | Meaning |
|---|---|---|
| 1 | 256-byte NUL-padded ASCII URL | Connect a URL image as the floppy/USB device |
| 2 | 256-byte URL | Connect a URL image as the CD/DVD |
| 3 | — | "No" (answers seize/share) |
| 4 | — | "Yes" |
| 5 | — | Disconnect the URL floppy |
| 6 | — | Disconnect the URL CD/DVD |
| 7 | 1456-byte `SMIF_0092` | Clipboard / file transfer packet |
| 8 | — | Start command-channel encryption (v1 `ENCRYPT_CMD`) |

### Clipboard and file transfer

This needs `REMOTE_CLIPBOARD_SUPPORT` and an HPE helper application running
in the host OS (see cmd 16). Payloads are packed structs:

- **Client → server** `SMIF_0092` (1456 bytes): `type` u8,
  `data[1447]`, `total_size` u32, `this_size` u32.
- **Server → client** `SMIF_0091` (1448 bytes, inside a cmd 12 packet):
  `type` u8, `data[1439]`, `total_size` u32, `this_size` u32.

Text and names are UTF-16LE. Large items are split across several packets.
`total_size` is the full length and `this_size` is the length in this packet.

| Type | Direction | Meaning |
|---|---|---|
| 1 | C→S | Clipboard text |
| 2 | S→C | Clipboard text |
| 3 | C→S | Offer a file: its name (UTF-16) |
| 4 | S→C | Keepalive for the client's file send |
| 5 | S→C | Ready, send the file data |
| 6 | both | Cancel a client→server file transfer |
| 7 | C→S | File data |
| 8 | C→S | Request to receive a file from the host |
| 9 | S→C | Keepalive for the client's file receive |
| 10 | S→C | Offered file name |
| 11 | C→S | Accept the offered file |
| 12 | both | Cancel a server→client file transfer |
| 13 | S→C | File data (complete when the bytes written equal `total_size`) |
| 14 | C→S | Clipboard image (bitmap data) |
| 15 | C→S | Cancel an image send |
| 16 | S→C | Clipboard image |
| 17 | S→C | Cancel an image send |
| 18 | C→S | Client machine name (UTF-16) |
| 19 | S→C | Host machine name (UTF-16) |

## 7. Virtual media

Each virtual drive gets its own TCP connection to `vm_port`.

### v1 handshake

There is no `'P'` greeting and no encryption: the stream is plain TCP. The
client sends 34 bytes:

| Offset | Size | Field |
|---|---|---|
| 0 | u8 | `0x10` |
| 1 | u8 | Device: 1 floppy, 2 CD/DVD, 3 USB key / virtual folder |
| 2 | 32 | Session key, ASCII. With `ENCRYPT_VMKEY` it is XORed with `enc_key_str` as in the negotiation. |

The server replies with 4 bytes. `20 00 ..` (`0x20`) means OK. Otherwise
`byte[0]` is the reason: `0x21` drive in use, `0x22` invalid key,
`0x23` not licensed, `0x24` key expired, `0x25` drive not configured,
`0x26` no permission.

In v2, the [hello](#v2-hello-every-channel) replaces this handshake, with
channel 2 floppy, 3 disc, 4 USB key or 5 folder, and the connection is
AES-OFB encrypted.

### Requests and replies

After the handshake, **the iLO acts as the SCSI initiator**:

1. The iLO sends a 12-byte CDB.
2. The client answers with a 16-byte `ReplyHeader`, followed by `length`
   bytes of data where the command returns data.
3. For writes (`WRITE_10/12`, `FORMAT_UNIT`), the client first reads the data
   that follows the CDB, then sends the header.

`ReplyHeader`:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | Magic `DE C0 AD 0B` (`0x0BADC0DE`) |
| 4 | u32 | Flags: bit 0 write-protected, bit 1 keepalive, bit 2 disconnect |
| 8 | u8 | Media type (0 = none, 1 = CD present, `0x24` = floppy present) |
| 9 | u8 | Sense key |
| 10 | u8 | ASC |
| 11 | u8 | ASCQ |
| 12 | u32 | Length of the data that follows |

Fields inside the CDBs are big-endian. Logical blocks are 512 bytes for
floppy and USB devices and 2048 bytes for CD. Once the link has been idle
for about 4 s, the client sends a keepalive header (flag bit 1) about once a
second.

Commands the client handles: TEST UNIT READY, REQUEST SENSE,
READ CAPACITY(10), READ FORMAT CAPACITIES, READ(10/12), WRITE(10/12),
WRITE AND VERIFY, FORMAT UNIT, MODE SENSE(10), PREVENT/ALLOW MEDIUM REMOVAL,
START STOP UNIT (eject ends the connection), READ TOC (formats 0 and 1),
GET EVENT STATUS NOTIFICATION, and SEND DIAGNOSTIC. Anything else gets
sense 5/0x24 (illegal request, invalid field in CDB).

Media changes follow the usual sequence: "not present" (2/0x3A, or 2/0x04
for floppy), then "medium changed" (6/0x28), then ready. A CDB whose first
byte is `0xFE` looks like a sync request. The client tries to echo it in a
header, but its copy has the wrong bounds and fails, so that path is
unreliable.

A virtual folder is presented as a synthesized FAT16 USB image
(`FAT_16`).

## Shared sessions

Shared sessions are peer-to-peer. The iLO only brokers the request:

1. The joining client asks to share (negotiation `0x56`, or v2 Command 2).
   On success it **listens** on `rc_port` (IPv4 and IPv6) for up to 10 s.
2. The current holder (the leader) receives command-channel cmd 9. If the
   user accepts, the leader sends "yes" and **connects to the joiner's
   address on `rc_port`**.
3. The leader forwards every decrypted KVM chunk it receives to each joiner,
   as plaintext. Each joiner decodes with no cipher.
4. Joiners send 10-byte messages to the leader. The u16 type is 1 or 2 for
   keyboard or mouse (the leader forwards these to the iLO), 3 for "closing"
   and 4 for "opened". The leader reports joins and leaves to the iLO with
   KVM types `0x0E`/`0x10` and `0x0D`/`0x11`.

ServerRemote probably won't need this. It is recorded here because a leader
must at least answer cmd 9 with "no", or the iLO waits out the timeout.

## Playback and capture files

The iLO keeps two server-side video capture buffers, "startup" and
"pre-failure". KVM `08 00` or `09 00` requests one. The iLO then streams it
over the command channel as cmd 8 packets:

- `seq` 0 starts the file. `flags & 7` says which buffer: 0 startup,
  1 pre-failure.
- `seq` `0xFFFF` ends it.
- The data is concatenated as it arrives.

The resulting file is an `ILO3_CAPTURE_HEADER` (332 bytes, packed) followed
by records. The header:

| Field | Size | Value |
|---|---|---|
| marker | u32 | `0x69966996` |
| version | u16 | `0x8000` |
| reserved | u16 | |
| datetime | u32 | Packed date, below |
| reserved2 | 64 bytes | |
| title | 256 bytes | ASCII |

Each record in the downloaded data is `0x5AA55AA5`, then a u32 length, a u32
timestamp in ms, and the data. The client strips the markers, and its own
recordings use the same format without them. Each record's data is raw,
already-decrypted KVM stream, fed through the same decoder.

`datetime` is packed as follows, from the most significant end:

| Field | Bits | Notes |
|---|---|---|
| year − 1990 | rest | |
| month | 4 | |
| day | 5 | |
| hour | 5 | |
| minute | 6 | |
| second | 6 | |

1990-01-01 means "iLO clock not set".

## Timeouts and liveness

- **KVM.** If no data arrives for 45 s (600 s for a shared viewer), the
  client assumes the connection is dead.
- **Virtual media.** Once idle, a keepalive header goes out about once a
  second.
- **Close.** The client sends KVM `0B 00`, closes the sockets and then
  logs out over HTTPS.

## Verified against a live iLO 4

Probed on 2026-11-18 against iLO 4 firmware 2.82, from throwaway scripts
(nothing in the repo). The probe changed nothing on the server. It sent only
login, `rc_info`, the two negotiations, `08 00 00 00` on the command channel,
one KVM refresh (`05 00`), and logout.

| Area | Result |
|---|---|
| Web API | `POST /json/login_session` returns the documented `SessionInfo` and sets a `sessionKey` cookie. `GET /json/session_info` works with that cookie. Logout returns 200, and `session_info` returns 403 afterwards. |
| Version detection | Redfish `RcInfo` gives 404 and `rc_info` reports `protocol_version` `"1.1"`, so v1 |
| KVM negotiation | Greeting `'P'`. The request `0x6001` (`0x2001 \| 0x4000`, because `ENCRYPT_KEY` and `ENCRYPT_VMKEY` were both set) with the XOR-obfuscated key was accepted with `'R'`. |
| Command negotiation | `0x6002` was accepted with `'R'` while the KVM connection was open |
| Session header and cipher switch | As described in [v1 KVM cipher selection](#v1-kvm-cipher-selection): a 7-byte plaintext header choosing RC4, then a fresh keystream from the next byte |
| Video codec | The scratch decoder written from [section 5](#5-kvm-server--client-video) decoded about 13 KB into a correct 1600×900 console image. It found 29 end-of-frame commands, each followed by a `TIMEOUT`, and no decode errors after the initial resync. |
| Client→server RC4 | A refresh (`05 00`) encrypted from the start of the client's RC4 keystream brought a full frame (about 12.8 KB) roughly 1.2 s later |
| Command channel | After `08 00 00 00`, AES-OFB with `cmd_enc_key` and a zero IV. The packets decoded as cmd 3 (power, `01` on) and cmd 4 (health, `01` caution), each sent twice. |

The test screen was grey text on black, so it couldn't show whether red and
blue are the right way round in the colour layout. It also sent no keyboard
or mouse input.

## Testing during implementation

The connector (`connectors/ilo`, `server-remote-ilo`) implements the v1 path
of this page. It is tested in two ways:

- **In CI**, against a mock iLO in `connectors/ilo/src/tests.rs`: an HTTPS
  server with a self-signed certificate for the web API, and a console port
  that checks the negotiation, sends the 7-byte session header that firmware
  2.82 sent, and then a small synthetic screen encrypted with fixed test
  keys. It also sends power and health over an AES command channel, and
  checks the keyboard, mouse and closing messages the client sends back.
- **Locally**, an ignored test decodes a real capture:
  `ILO_CAPTURE=<capture>.json cargo test -p server-remote-ilo -- --ignored`
  (add `ILO_CAPTURE_PPM=<file>` to save the screen). The captures stay
  outside the repo, as described below. On `kvm_refresh.json` it decodes a
  correct 1600×900 screen.

The plan was to capture a real iLO 4's behaviour and build the mock from
it, so the connector can be developed and tested in CI without hardware.

- **Capture.** Record each scenario from a real iLO 4: the HTTPS
  request/response pairs, then the raw bytes of each TCP connection in both
  directions, with timing and `recv()` boundaries. The safe, read-only
  scenarios were recorded on 2026-11-18 from firmware 2.82 and are kept
  **outside the repo**, in `~/reveng/hpe-ilo/captures/` on the author's
  machine. Its README lists the scenarios and the file format. The recorder
  (`capture.py`) and a reference Python decoder (`dvc.py`) are in `tools/`
  beside them.
  - Captured scenarios: web API, idle KVM, KVM refresh, the command channel
    with and without encryption, a busy second viewer, and a bad session key.
  - Record with a dedicated test account and a disposable console screen.
  - **Captures hold secrets.** Session keys, `enc_key`, `cmd_enc_key` and
    screen contents are all in them. Re-key them before committing anything:
    decrypt with the recorded keys, replace the keys with fixed test keys, and
    re-encrypt.
- **Mock server.** The mock serves the recorded fixtures:
  - `/json/login_session`, `/json/session_info` and `/json/rc_info` over
    HTTPS, with Redfish `RcInfo` returning 404;
  - the `'P'` greeting and the status byte on `rc_port`;
  - the session header followed by recorded video, encrypted with the test
    keys;
  - command-channel packets such as power state and health.

  It also checks what the client sends: negotiation requests, the `ENCRYPT_CMD`
  switch, and refresh, keyboard and mouse messages. Scripted variants cover
  the paths that are risky or rare on real hardware: a busy console
  (`'S'`/`'Y'`), seize and share prompts (cmd 6 and 9), power changes, POST
  codes, and an iLO reset (cmd 15).
- **Settling the open questions.** Each item below can be answered by
  capturing the scenario once, on a test server where it is safe, and adding
  it to the mock's fixtures.

## Open questions

These still need a live test. The plan is to settle them during
implementation, as described in
[Testing during implementation](#testing-during-implementation):

1. **Colour channel order.** Grey output can't tell red from blue. Check with
   a coloured screen, such as the BIOS setup.
2. **The power opcode.** `SendPower` writes type `0x0000`, which stands out
   next to the other numbered types. The decompile may hide an assignment.
   Testing it presses the server's power button, so it needs a server that
   can be power-cycled.
3. **Keyboard and mouse.** The layouts are from the decompile only.
4. **The 3- and 2-bit colour layouts** (see [Colours](#state-machine)).
   Testing them means asking for a lower colour depth (`07 00 mode`).
5. **Virtual media**, including the `0xFE` sync request. The probe never
   opened `vm_port`.
