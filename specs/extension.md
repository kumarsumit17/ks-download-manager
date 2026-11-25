---
type: Specification
title: Chrome extension and hand-off
description: What the Chrome extension does, its permissions, and how it hands links (and, for browser downloads, the cookies they need) to the desktop app over native messaging restricted to our extension ID.
tags: [extension, chrome, native-messaging, security]
---

# Chrome extension and hand-off

The extension hands links to the app; all downloading stays in the Rust app. For the video and image buttons only the link is sent, so only publicly reachable media works there. For a download Chrome starts (Browser downloads, below) the site's cookies for that file, the page it came from and Chrome's identification go with the link, like IDM's extension, so files that need the browser's sign-in work too. Page content is never sent. It is loaded unpacked (not published on the Chrome Web Store); install steps are in the README.

## What it does
- **YouTube:** on watch pages and Shorts, one floating **Download** button (bottom right). One click sends `https://www.youtube.com/watch?v=<id>` (playlist, time and tracking parameters dropped); the app downloads the best available quality ([video-playlists](video-playlists.md)). youtu.be links open a watch page, where the button appears. Right-clicking a link to a YouTube video or a youtu.be link on any page offers **Download video with KS Download Manager**. The button floats instead of sitting in YouTube's toolbar, because YouTube's page structure changes often; it depends only on the address.
- **X (x.com, twitter.com):** hovering an image shown at least 100×100 px shows **Download**, which sends the address of the largest version (`name=orig` on `pbs.twimg.com`; see `shared/extension/src/site-rules.ts`).
- **Other sites:** the same hover button, **off by default**. The user turns it on for one site in the toolbar popup, which registers the hover script for that site alone (the switch shows whether that site's script is registered). The largest candidate is chosen from standard HTML only: a link around the image pointing straight at an image file, else the widest `srcset` candidate (or highest density), else the image's own address. `data:` and `blob:` images are skipped. The image under the pointer is looked up (`elementsFromPoint`, because sites lay transparent elements over images) at most once per animation frame.
- **Browser downloads (on by default, like IDM):** every download Chrome starts goes to the app, where it is tracked, paused and resumed like any other; the popup's **Send browser downloads to the app** switch turns this off (`handOffDownloads: false`, saved by the background worker so it holds even if the popup closes). The `onCreated` listener is registered when the worker starts. Chrome's download is paused while the app checks the link; if the app takes it, Chrome's copy is cancelled and removed from Chrome's list, otherwise Chrome resumes it.
  - **Sent with it:** the site's cookies for the file's link (`chrome.cookies.getAll({url})`, joined as one `Cookie` value, at most 16 KB), the page the download came from (`referrer`, http(s) only) and Chrome's `navigator.userAgent`, as `headers` on the `add` message (`kind: file` only). Anything that is not printable ASCII is left out. The app sends the cookies only to the link's own host (never to a host a redirect leads to), uses all three only for that download, and keeps them in memory: they are never written to disk, so after an app restart an unfinished download resumes without them.
  - **Stays in Chrome:** a download Chrome flags (`danger` other than `safe`, `accepted`, `allowlistedByPolicy`, `deepScannedSafe` or none yet; read again after pausing), downloads in private windows, ones another extension started, `blob:` and `data:` files, and any the app refuses (for example a link that answers with a web page). Chrome rates most files only after creating the download, so an unrated download is handed over; the app marks every file it saves as downloaded from the internet, so the system still checks it.
  - **What happened:** the outcome of the last download (`lastHandOff`: sent, or the reply code) is shown in the popup as "Last download (site): sent to the app" or why it stayed in Chrome.
  - Earlier versions required a `danger` of `safe` at creation, which Chrome rarely reports yet, and saved the switch only after Chrome's permission prompt, which closes the popup: so nothing was ever handed over. Both are fixed by the rules above.

## Permissions
| Permission | Why |
|---|---|
| `nativeMessaging` | Talk to the app |
| `scripting` | Register the hover script for a site the user enabled |
| `storage` | Remember the browser-downloads switch and the last outcome |
| `activeTab` | Read the current tab's address in the popup, only when the user opens it |
| `contextMenus` | The menu item on YouTube links |
| `downloads` | Hand Chrome's downloads to the app (on by default) |
| `cookies` | Read the site's cookies for a download being handed over, for that download only |
| Host: `https://*/*`, `http://*/*` | Needed to read a site's cookies for its downloads; also covers the built-in buttons and the per-site image button. Chrome shows "Read and change all your data on all websites". |

No `externally_connectable`: web pages cannot message the extension. The background worker accepts messages only from its own scripts (`sender.id`). The page button lives in a closed shadow root and reacts only to trusted clicks (`event.isTrusted`), so a page cannot press it.

## Hand-off: native messaging
Compared: a custom URL scheme (any website or program can trigger it, no answer comes back) and a localhost port (any website can send requests to it, DNS rebinding, a token to manage, firewall prompts). Native messaging was chosen: Chrome itself enforces which extension may connect.

- **Host manifest:** on every start the app writes `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.ks.downloadmanager.json` (only when Chrome's folder exists), with `allowed_origins` set to our one extension, `chrome-extension://ikplfdbdfikcokpegkapjdedlgifmkbe/`. The ID is fixed by the public `key` in `shared/extension/manifest.json`; no private key is kept.
- **Host mode:** Chrome starts the app's own executable with the caller's origin as its argument. The host refuses any origin but ours, exactly, before reading anything; then reads Chrome's framed messages (4-byte native-order length, JSON, at most 8 KB), checks them, and forwards each to the running app.
- **Messages:** `{"type":"add","v":1,"kind":"video"|"image"|"file","url":…}`, `ping`, `show`. Unknown fields are rejected. URLs must be http(s), at most 2048 characters, without a user name or password. The app also checks the kind against the link: an `image` must be an image file, a `file` must not be a web page, a `video` must be a video page, and a playlist link is refused with `playlist` (playlists are chosen in the app's picker, Part 5). Replies carry only a code (`err_<code>` in `_locales`), never a sentence, and a reply to `ping` may carry `update`: a newer app version the app's own update check found, which the popup shows with a link to the latest release ([UI direction](ui.md), "App updates"; the extension itself never checks the network); the extension shows it in the browser's language. Labels are in English and Hindi (`_locales/en`, `_locales/hi`), checked with the app's language files.
- **Socket:** the app listens on `ext.sock` in its app data folder (or `/tmp/ksdm-<uid>/ext.sock` when that path would pass the 104-byte Unix limit). The `/tmp` folder is used only if it is a real folder (not a link), owned by the current user, with mode `0700`; the app creates it that way when missing, never changes an existing one, and both the app and the host refuse it otherwise. The socket is `0600`, and a connection from any other user ID is not answered. A connection that sends no complete line within 5 s is closed.
- **Single instance:** binding the socket is the app's single-instance lock. If the socket file exists, the app first connects to it: if a copy answers, the new one asks it to show its window and exits; only if nobody answers is the stale file (only ever a socket, never a regular file) removed and bound again.
- **App not running:** the host starts it through LaunchServices (`open -g -b com.ks.downloadmanager`), so it is not a child of Chrome, and waits up to 10 s for the socket.
- **What remains possible:** another program running as the same user can connect to the socket and add a download, as it could also edit the app's own files; that is outside what an app can defend against.

## Not yet
- Windows: the registry entry for the host manifest, a named pipe instead of the socket, and starting the app (Release stage).
- Edge, Brave and other Chromium browsers keep host manifests in other folders; only Chrome is set up.
