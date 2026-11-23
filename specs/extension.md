---
type: Specification
title: Chrome extension and hand-off
description: What the Chrome extension does, its permissions, and how it hands URLs to the desktop app over native messaging restricted to our extension ID.
tags: [extension, chrome, native-messaging, security]
---

# Chrome extension and hand-off

The extension only hands over URLs. All downloading stays in the Rust app. It never sends cookies, page content or the address of the page the user is on, so only publicly reachable media works. It is loaded unpacked (not published on the Chrome Web Store); install steps are in the README.

## What it does
- **YouTube:** on watch pages and Shorts, one floating **Download** button (bottom right). One click sends `https://www.youtube.com/watch?v=<id>` (playlist, time and tracking parameters dropped); the app downloads the best available quality ([video-playlists](video-playlists.md)). youtu.be links open a watch page, where the button appears. Right-clicking a link to a YouTube video or a youtu.be link on any page offers **Download video with KS Download Manager**. The button floats instead of sitting in YouTube's toolbar, because YouTube's page structure changes often; it depends only on the address.
- **X (x.com, twitter.com):** hovering an image shown at least 100×100 px shows **Download**, which sends the address of the largest version (`name=orig` on `pbs.twimg.com`; see `extension/src/site-rules.ts`).
- **Other sites:** the same hover button, **off by default**. The user turns it on for one site in the toolbar popup, which asks Chrome for that one site's permission (optional host permission) and registers the hover script for it alone. The largest candidate is chosen from standard HTML only: a link around the image pointing straight at an image file, else the widest `srcset` candidate (or highest density), else the image's own address. `data:` and `blob:` images are skipped. The image under the pointer is looked up (`elementsFromPoint`, because sites lay transparent elements over images) at most once per animation frame.
- **Browser downloads (optional, off by default):** the popup can send ordinary Chrome downloads to the app. Turning it on asks for the optional `downloads` permission (Chrome shows "Manage your downloads"). Chrome's download is paused while the app checks the link; if the app takes it, Chrome's copy is cancelled and removed from Chrome's list, otherwise Chrome resumes it. A download Chrome marks as dangerous (`danger` other than `safe` or `accepted`), or is still scanning, is never taken over; the verdict is read again after pausing. Downloads that need a sign-in, a form post or a `blob:` address stay in Chrome. The option's description tells the user that Chrome's download safety check does not apply to files sent to the app, that downloads needing a sign-in stay in Chrome, and that single-use links may fail (the app fetches the link a second time).

## Permissions
| Permission | Why |
|---|---|
| `nativeMessaging` | Talk to the app |
| `scripting` | Register the hover script for a site the user enabled |
| `storage` | Remember the browser-downloads switch |
| `activeTab` | Read the current tab's address in the popup, only when the user opens it |
| `contextMenus` | The menu item on YouTube links |
| Host: `https://www.youtube.com/*`, `https://x.com/*`, `https://twitter.com/*` | The built-in buttons |
| Optional host: `https://*/*`, `http://*/*` | Granted one site at a time, only when the user enables the image button there |
| Optional: `downloads` | Only when the user turns on the browser-downloads hand-off |

No `externally_connectable`: web pages cannot message the extension. The background worker accepts messages only from its own scripts (`sender.id`). The page button lives in a closed shadow root and reacts only to trusted clicks (`event.isTrusted`), so a page cannot press it.

## Hand-off: native messaging
Compared: a custom URL scheme (any website or program can trigger it, no answer comes back) and a localhost port (any website can send requests to it, DNS rebinding, a token to manage, firewall prompts). Native messaging was chosen: Chrome itself enforces which extension may connect.

- **Host manifest:** on every start the app writes `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.ks.downloadmanager.json` (only when Chrome's folder exists), with `allowed_origins` set to our one extension, `chrome-extension://ikplfdbdfikcokpegkapjdedlgifmkbe/`. The ID is fixed by the public `key` in `extension/manifest.json`; no private key is kept.
- **Host mode:** Chrome starts the app's own executable with the caller's origin as its argument. The host refuses any origin but ours, exactly, before reading anything; then reads Chrome's framed messages (4-byte native-order length, JSON, at most 8 KB), checks them, and forwards each to the running app.
- **Messages:** `{"type":"add","v":1,"kind":"video"|"image"|"file","url":…}`, `ping`, `show`. Unknown fields are rejected. URLs must be http(s), at most 2048 characters, without a user name or password. The app also checks the kind against the link: an `image` must be an image file, a `file` must not be a web page, a `video` must be a video page, and a playlist link is refused with `playlist` (playlists are chosen in the app's picker, Part 5). Replies carry only a code (`err_<code>` in `_locales`), never a sentence, and a reply to `ping` may carry `update`: a newer app version the app's own update check found, which the popup shows with a link to the latest release ([UI direction](ui.md), "App updates"; the extension itself never checks the network); the extension shows it in the browser's language. Labels are in English and Hindi (`_locales/en`, `_locales/hi`), checked with the app's language files.
- **Socket:** the app listens on `ext.sock` in its app data folder (or `/tmp/ksdm-<uid>/ext.sock` when that path would pass the 104-byte Unix limit). The `/tmp` folder is used only if it is a real folder (not a link), owned by the current user, with mode `0700`; the app creates it that way when missing, never changes an existing one, and both the app and the host refuse it otherwise. The socket is `0600`, and a connection from any other user ID is not answered. A connection that sends no complete line within 5 s is closed.
- **Single instance:** binding the socket is the app's single-instance lock. If the socket file exists, the app first connects to it: if a copy answers, the new one asks it to show its window and exits; only if nobody answers is the stale file (only ever a socket, never a regular file) removed and bound again.
- **App not running:** the host starts it through LaunchServices (`open -g -b com.ks.downloadmanager`), so it is not a child of Chrome, and waits up to 10 s for the socket.
- **What remains possible:** another program running as the same user can connect to the socket and add a download, as it could also edit the app's own files; that is outside what an app can defend against.

## Not yet
- Windows: the registry entry for the host manifest, a named pipe instead of the socket, and starting the app (Release stage).
- Edge, Brave and other Chromium browsers keep host manifests in other folders; only Chrome is set up.
