// Site rules: everything that depends on how one particular site builds its pages or names
// its files. Sites change, so this is the one file to update when they do. Written from
// observing the sites' own public pages and image addresses (specs/ui.md, clean room),
// never from another extension's code.

// ---- YouTube ---------------------------------------------------------------------------

const VIDEO_ID = /^[A-Za-z0-9_-]{11}$/;

/**
 * The canonical address of a single YouTube video for a watch page, a Shorts page or a
 * youtu.be link; null for anything else. Playlist, time and tracking parameters are
 * dropped, so the app downloads exactly that one video.
 */
export function youtubeVideoUrl(href: string): string | null {
  let u: URL;
  try {
    u = new URL(href);
  } catch {
    return null;
  }
  if (u.protocol !== "https:" && u.protocol !== "http:") return null;
  const host = u.hostname.toLowerCase();
  let id: string | null = null;
  if (host === "youtu.be") {
    id = u.pathname.slice(1).split("/")[0] ?? null;
  } else if (host === "youtube.com" || host === "www.youtube.com" || host === "m.youtube.com") {
    if (u.pathname === "/watch") id = u.searchParams.get("v");
    else if (u.pathname.startsWith("/shorts/")) id = u.pathname.split("/")[2] ?? null;
  }
  return id && VIDEO_ID.test(id) ? `https://www.youtube.com/watch?v=${id}` : null;
}

/** Link patterns for the "download this video" menu on links (Chrome match patterns). */
export const YOUTUBE_LINK_PATTERNS = [
  "https://www.youtube.com/watch*",
  "https://youtube.com/watch*",
  "https://m.youtube.com/watch*",
  "https://www.youtube.com/shorts/*",
  "https://youtu.be/*",
];

// ---- X (x.com, twitter.com) ------------------------------------------------------------

/** Hosts where the image button is built in (no opt-in needed). */
export const X_HOSTS = ["x.com", "twitter.com"];

/**
 * The largest version of an image on X's image server, from the address the page shows.
 * Observed address forms:
 * - `https://pbs.twimg.com/media/<id>?format=jpg&name=small` (also `medium`, `large`,
 *   `900x900`, `360x360`, …): `name=orig` is the original upload.
 * - Older `https://pbs.twimg.com/media/<id>.jpg` and `…/<id>.jpg:large`: the same original
 *   as `?format=jpg&name=orig`.
 * - Video previews (`ext_tw_video_thumb`, `amplify_video_thumb`, `tweet_video_thumb`) take
 *   the same `name` parameter.
 * - Profile pictures: `…/profile_images/<n>/<file>_normal.jpg` (also `_bigger`, `_mini`,
 *   `_200x200`, `_400x400`); without the suffix it is the uploaded size.
 * Anything else is returned unchanged.
 */
export function xLargestImage(src: string): string {
  let u: URL;
  try {
    u = new URL(src);
  } catch {
    return src;
  }
  if (u.hostname !== "pbs.twimg.com") return src;
  const path = u.pathname;
  if (path.startsWith("/profile_images/")) {
    u.pathname = path.replace(/_(normal|bigger|mini|reasonably_small|\d+x\d+)(\.[A-Za-z0-9]+)$/, "$2");
    return u.toString();
  }
  const media = /^\/(media|ext_tw_video_thumb|amplify_video_thumb|tweet_video_thumb)\//.test(path);
  if (!media) return src;
  // Old form: extension (and maybe ":size") in the path.
  const old = path.match(/^(.*\/[^/.:]+)\.([A-Za-z0-9]+)(?::[a-z0-9]+)?$/);
  if (old) {
    u.pathname = old[1];
    u.search = "";
    u.searchParams.set("format", old[2].toLowerCase());
    u.searchParams.set("name", "orig");
    return u.toString();
  }
  if (u.searchParams.has("format") || u.searchParams.has("name")) {
    u.searchParams.set("name", "orig");
    return u.toString();
  }
  return src;
}
