// The largest version of an image a page offers, from standard HTML only (no site rules):
// the `srcset` of the image or of its <picture> sources, or a link around the image that
// points straight at an image file. Follows the HTML `srcset` syntax: comma-separated
// candidates, each a URL and an optional `<n>w` (width) or `<n>x` (density) descriptor.

export interface Candidate {
  url: string;
  /** Width in pixels (`w` descriptor), when given. */
  width?: number;
  /** Pixel density (`x` descriptor), 1 when no descriptor is given. */
  density?: number;
}

/** Parses a `srcset` value. URLs are resolved against `base`. Bad entries are skipped. */
export function parseSrcset(srcset: string, base: string): Candidate[] {
  const out: Candidate[] = [];
  let i = 0;
  const s = srcset;
  while (i < s.length) {
    // Skip whitespace and stray commas between candidates.
    while (i < s.length && (s[i] === "," || /\s/.test(s[i]))) i++;
    if (i >= s.length) break;
    let j = i;
    while (j < s.length && !/\s/.test(s[j])) j++;
    let url = s.slice(i, j);
    let descriptor = "";
    if (url.endsWith(",")) {
      // A URL directly followed by a comma has no descriptor.
      url = url.replace(/,+$/, "");
    } else {
      let k = j;
      while (k < s.length && s[k] !== ",") k++;
      descriptor = s.slice(j, k).trim();
      j = k;
    }
    i = j + 1;
    const resolved = resolve(url, base);
    if (!resolved) continue;
    const m = descriptor.match(/^(\d+(?:\.\d+)?)([wx])$/i);
    if (descriptor === "") out.push({ url: resolved, density: 1 });
    else if (m && m[2].toLowerCase() === "w") out.push({ url: resolved, width: Number(m[1]) });
    else if (m) out.push({ url: resolved, density: Number(m[1]) });
  }
  return out;
}

function resolve(url: string, base: string): string | null {
  if (!url) return null;
  try {
    const u = new URL(url, base);
    return u.protocol === "https:" || u.protocol === "http:" ? u.toString() : null;
  } catch {
    return null;
  }
}

/** The best of the candidates: the largest width, else the largest density. */
export function pickLargest(candidates: Candidate[]): string | null {
  const withWidth = candidates.filter((c) => c.width !== undefined);
  if (withWidth.length > 0) return withWidth.reduce((a, b) => ((b.width ?? 0) > (a.width ?? 0) ? b : a)).url;
  if (candidates.length === 0) return null;
  return candidates.reduce((a, b) => ((b.density ?? 1) > (a.density ?? 1) ? b : a)).url;
}

const IMAGE_FILE = /\.(jpe?g|png|gif|webp|avif|bmp|tiff?|heic|heif)$/i;

/** True when `href` points straight at an image file (judged by its path). */
export function isImageFileUrl(href: string): boolean {
  try {
    const u = new URL(href);
    return (u.protocol === "https:" || u.protocol === "http:") && IMAGE_FILE.test(u.pathname);
  } catch {
    return false;
  }
}

/** What the page says about one image, gathered by the content script. */
export interface ImageFacts {
  base: string;
  currentSrc: string;
  srcset: string | null;
  /** `srcset` values of the <source> elements of an enclosing <picture>. */
  sources: string[];
  /** `href` of the nearest enclosing link, if any. */
  linkHref: string | null;
}

/**
 * The address to hand over for an image: a linked full-size image file first (that is what
 * the link is there for), else the largest `srcset` candidate, else the image's own
 * address. Data and blob addresses are never returned: they cannot be fetched by the app.
 */
export function largestImage(f: ImageFacts): string | null {
  if (f.linkHref && isImageFileUrl(f.linkHref)) return f.linkHref;
  const candidates = [f.srcset, ...f.sources].filter((s): s is string => !!s).flatMap((s) => parseSrcset(s, f.base));
  const best = pickLargest(candidates);
  if (best) return best;
  return resolve(f.currentSrc, f.base);
}
