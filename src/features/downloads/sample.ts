import type { Download } from "./types";

// Static rows for Stage 1 only. Replaced by engine state in Stage 4.
const now = Date.now();
const MB = 1024 * 1024;

export const sampleDownloads: Download[] = [
  { id: "1", name: "holiday-2026.mp4", url: "https://example.com/media/holiday-2026.mp4", category: "video", status: "downloading", size: 1229 * MB, received: 786 * MB, speed: 18.4 * MB, connections: 16, queue: "Main queue", addedAt: now - 4 * 60_000 },
  { id: "2", name: "album-lossless.zip", url: "https://example.com/files/album-lossless.zip", category: "audio", status: "paused", size: 310 * MB, received: 122 * MB, speed: 0, connections: 0, queue: "Main queue", addedAt: now - 55 * 60_000 },
  { id: "3", name: "cover.png", url: "https://example.com/img/cover.png", category: "images", status: "finished", size: 2.1 * MB, received: 2.1 * MB, speed: 0, connections: 0, queue: "Main queue", addedAt: now - 3 * 3_600_000 },
  { id: "4", name: "ubuntu-26.04-desktop-amd64.iso", url: "https://example.com/iso/ubuntu.iso", category: "other", status: "downloading", size: 6.1 * 1024 * MB, received: 1.4 * 1024 * MB, speed: 42.7 * MB, connections: 24, queue: "Main queue", addedAt: now - 9 * 60_000 },
  { id: "5", name: "podcast-episode-112.mp3", url: "https://example.com/pod/112.mp3", category: "audio", status: "queued", size: 84 * MB, received: 0, speed: 0, connections: 0, queue: "Main queue", addedAt: now - 2 * 60_000 },
  { id: "6", name: "lecture-07.webm", url: "https://example.com/v/lecture-07.webm", category: "video", status: "failed", size: null, received: 12 * MB, speed: 0, connections: 0, queue: "Main queue", addedAt: now - 26 * 3_600_000 },
  { id: "8", name: "live-concert-recording.ts", url: "https://example.com/live/concert.ts", category: "video", status: "downloading", size: null, received: 412 * MB, speed: 6.2 * MB, connections: 1, queue: "Main queue", addedAt: now - 7 * 60_000 },
  { id: "7", name: "wallpapers.zip", url: "https://example.com/files/wallpapers.zip", category: "images", status: "finished", size: 148 * MB, received: 148 * MB, speed: 0, connections: 0, queue: "Main queue", addedAt: now - 50 * 3_600_000 },
];
