# Video and playlist downloads

## What the user can do
- Paste a link to a single video or a whole playlist from YouTube or another supported video site.
- For a playlist: see the list of entries with title, duration and thumbnail, choose quality once for all, and start. Every entry is selected when the list opens, so downloading the whole playlist needs no ticking at all.
- A **Select all** box at the top of the list ticks or unticks every entry in one click, with a count beside it ("42 of 42 selected"). Individual boxes are only for leaving videos out. Shift-click selects a range.
- Choose format per download: video (resolution choice) or audio only.
- Each playlist entry appears as its own row in the download list, grouped under the playlist name, saved to a folder named after the playlist, with an optional number prefix to keep the order.
- Pause, resume and retry work per entry and for the whole playlist. Entries that are unavailable (private, removed, region-blocked) are marked and skipped, without stopping the rest.

## How it works
- Site support comes from **yt-dlp**, run as a separate bundled program (Tauri sidecar) that the Rust engine calls for two jobs: listing a link's entries and formats, and fetching the media. **ffmpeg** is used to join separate video and audio streams.
- Reason: every video site changes often and needs constant extractor updates. Writing and maintaining our own extractors for many sites is not realistic; yt-dlp is the maintained, openly licensed tool for this. It is used unmodified as a dependency, which is consistent with [clean-room](clean-room.md).
- The app can update yt-dlp on its own, separately from app releases, because sites break old versions quickly.
- Ordinary file downloads (images, archives, direct media links) do not go through yt-dlp; they use our own segmented Rust engine.

## Limits
- No DRM-protected content (paid streaming services). The app does not attempt to remove or bypass copy protection.
- Private or sign-in-only videos are out of scope for the first version.
- The user is responsible for having the right to download what they request. The first-run screen and the README say so plainly.

## Open points
- Licences to confirm and record in `docs/THIRD_PARTY.md` before bundling: yt-dlp and the exact ffmpeg build.
- Chrome Web Store rules on extensions that help download YouTube video: confirm before designing the extension's "send to app" button for video pages.
