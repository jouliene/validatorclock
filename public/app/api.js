// Without a deadline a hung request never settles, and the callers that share
// an in-flight request would then wait on it for the life of the page: the
// clock and the round stats both stop refreshing. The server gives up on a
// request long before this.
const FETCH_TIMEOUT_MS = 15000;

function fetchDeadline(timeoutMs) {
  return typeof AbortSignal !== "undefined" && AbortSignal.timeout
    ? AbortSignal.timeout(timeoutMs)
    : undefined;
}

async function fetchJson(url, timeoutMs = FETCH_TIMEOUT_MS) {
  const response = await fetch(url, {
    headers: { Accept: "application/json" },
    signal: fetchDeadline(timeoutMs)
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(body.error || `${response.status} ${response.statusText}`);
  }
  return body;
}
