// Drag and drop onto the window (per-platform plan, step 2): a link dragged from a browser
// or another app (`text/uri-list`, else plain text), or a `.webloc` (macOS) or `.url`
// (Windows) file from Finder or Explorer. Only one http(s) link is taken; anything else is
// ignored. Plain functions, so they are tested without a browser.

/** The first http(s) link in a `text/uri-list` (comment lines start with `#`) or in text. */
export function linkFromText(text: string): string | null {
  for (const line of text.split(/\r?\n/)) {
    const t = line.trim();
    if (!t || t.startsWith("#")) continue;
    return isWebLink(t) ? t : null;
  }
  return null;
}

/** The link inside a `.webloc` (XML property list) or `.url` (`URL=` line) file. */
export function linkFromShortcut(fileName: string, contents: string): string | null {
  const name = fileName.toLowerCase();
  if (name.endsWith(".webloc")) {
    const m = contents.match(/<key>URL<\/key>\s*<string>([^<]+)<\/string>/);
    const url = m ? unescapeXml(m[1].trim()) : "";
    return isWebLink(url) ? url : null;
  }
  if (name.endsWith(".url")) {
    const m = contents.match(/^\s*URL\s*=\s*(.+?)\s*$/im);
    return m && isWebLink(m[1]) ? m[1] : null;
  }
  return null;
}

function unescapeXml(s: string): string {
  return s.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&apos;/g, "'").replace(/&amp;/g, "&");
}

function isWebLink(s: string): boolean {
  if (s.length > 2048 || /\s/.test(s)) return false;
  try {
    const u = new URL(s);
    return (u.protocol === "https:" || u.protocol === "http:") && u.hostname !== "" && u.username === "" && u.password === "";
  } catch {
    return false;
  }
}
