/** "just now", "5 min ago", "3 h ago", "2 days ago" relative to `now` (epoch ms). */
export function timeAgo(epochMs: number, now = Date.now()): string {
  const seconds = Math.round((now - epochMs) / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  return `${days} day${days === 1 ? "" : "s"} ago`;
}

/** "in 4 min" / "now" for an upcoming time. */
export function timeUntil(epochMs: number, now = Date.now()): string {
  const minutes = Math.round((epochMs - now) / 60000);
  if (minutes <= 0) return "now";
  if (minutes < 60) return `in ${minutes} min`;
  return `in ${Math.round(minutes / 60)} h`;
}

export function formatDateTime(epochMs: number): string {
  return new Date(epochMs).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** A file name that is safe on every OS. */
export function safeFileName(name: string): string {
  return name.replace(/[\\/:*?"<>|]+/g, "-").replace(/\s+/g, " ").trim() || "airdash";
}
