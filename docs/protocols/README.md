# Protocols

What Server Remote supports for each protocol, what the server needs, and
what to expect. Every protocol is still in progress; the
[roadmap](../architecture/roadmap.md) says what's coming next, and
[implementing a protocol](implementing.md) explains how new protocol work is
done.

| Protocol | Status | Connection | Authentication |
|---|---|---|---|
| [HPE iLO 4](#hpe-ilo-4) | In progress | HTTPS login, then the iLO's own encrypted console connections | iLO user name and password |
| [Spice](#spice) | In progress | Plain TCP | Spice password, or none |
| [VNC](#vnc) | In progress | Plain TCP | VNC password, or none |

## Feature matrix

✅ supported · ⚠️ partly, or not verified on real hardware · ❌ not yet ·
– not part of the protocol

| Feature | HPE iLO 4 | Spice | VNC |
|---|:-:|:-:|:-:|
| Display | ✅ | ⚠️ uncompressed only | ✅ |
| Keyboard | ⚠️ | ⚠️ US layout only | ✅ |
| Mouse | ⚠️ | ✅ | ✅ |
| Password authentication | ✅ | ✅ | ✅ |
| Encryption | ✅ HTTPS login, encrypted console | ❌ | ❌ |
| Compressed video | ✅ | ❌ | ❌ |
| Power control | ⚠️ | – | – |
| Power, health and POST status | ✅ | – | – |
| Take over or share a console in use | ❌ | – | – |
| Virtual media / USB redirection | ❌ | ❌ | – |
| Clipboard | ❌ | ❌ | ❌ |
| Audio | – | ❌ | – |
| Remote cursor shape | – | ❌ | ❌ |
| Multiple monitors | – | ❌ | ❌ |

The sections below explain each ⚠️ and ❌.

## HPE iLO 4

Server Remote implements the iLO Integrated Remote Console itself, without
HPE's .NET or Java clients.

**Works:** the console's video, keyboard and mouse; a Power menu; Ctrl+Alt+Del;
and the server's power, health and POST status in the status bar.

**Caveats:**

- **iLO 4 only.** It has been checked against iLO 4 firmware 2.82. iLO 5 and
  later negotiate the console differently, through Redfish, and aren't
  supported yet.
- **Not fully verified on hardware.** Video has been checked against a real
  iLO 4. Keyboard, mouse, colour accuracy and the power actions were built
  from protocol analysis and tested against a simulated iLO, so they may
  misbehave on real servers. Please report what you find.
- **One user at a time.** If someone else already has the remote console open,
  connecting fails with an error. Taking over or sharing their session isn't
  supported yet.
- **No virtual media** yet, so disk images can't be mounted from Server Remote.
- **No clipboard.**
- **Licensing is up to the iLO.** The iLO limits the remote console without
  an iLO Advanced licence. How Server Remote behaves on an unlicensed iLO
  hasn't been tested.
- **Certificates.** iLOs usually have self-signed certificates. The first
  connection shows the certificate's fingerprint and asks whether to trust it
  once, always or never; Settings › Security sets how strict this is. Older
  iLOs' 1024-bit RSA keys are accepted, but only for a certificate you've
  approved.

## Spice

For virtual machines running in QEMU, whether managed with libvirt or by
hand.

**Works:** display, keyboard and mouse.

**Server setup.** Server Remote connects to the Spice port directly, over
plain TCP, and needs image compression and video streaming turned off. With
QEMU on the command line:

```sh
-spice port=5930,image-compression=off,streaming-video=off
```

or in a libvirt domain:

```xml
<graphics type='spice' port='5930'>
  <image compression='off'/>
  <streaming mode='off'/>
</graphics>
```

Add a password with the usual QEMU or libvirt options if the port is
reachable by anyone else.

**Caveats:**

- **Uncompressed images only.** Server Remote doesn't decode compressed
  updates (LZ, GLZ, QUIC or JPEG) or video streams, and skips them, so parts
  of the screen stop updating. It asks the server for uncompressed images, but
  not every server honours that, so turn compression off on the server as
  above. Uncompressed updates use more bandwidth, which suits a local
  network best.
- **No TLS.** Only the plain `port` works, not `tls-port`. That also rules out
  Proxmox VE's built-in Spice consoles, which go through its TLS proxy. Keep
  the port on a trusted network, or reach it through an SSH tunnel.
- **US keyboard layout.** Keys are sent as US QWERTY scan codes, so set the
  guest to a US layout or some characters will come out differently.
- **Not supported yet:** clipboard, audio, USB redirection, the guest's cursor
  shape, multiple monitors and live migration.

## VNC

A well-established protocol, included as a reference to compare Server
Remote with other remote clients. It stays in progress until it matches them
in features and performance.

**Works:** display, keyboard and mouse.

**Caveats:**

- **Basic security only.** It supports no authentication or a VNC password,
  over plain TCP. TLS (VeNCrypt) and other authentication types aren't
  supported. macOS Screen Sharing, for example, only works once its legacy
  "VNC viewers may control screen with password" option is turned on.
- **Uncompressed updates.** Only the Raw and CopyRect encodings are used, so
  screen changes arrive uncompressed. That's fine on a local network and slow
  over anything else. ZRLE and Tight are on the roadmap.
- **Whole-screen redraws.** Each update redraws the whole display, which
  costs more CPU on large screens.
- **Not supported yet:** clipboard, the bell and the remote cursor shape.
