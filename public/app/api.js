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
  let response;
  try {
    response = await fetch(url, {
      headers: { Accept: "application/json" },
      signal: fetchDeadline(timeoutMs)
    });
  } catch (error) {
    // What the deadline throws is a DOMException whose message is "signal timed out",
    // and that went into the error banner verbatim.
    throw error?.name === "TimeoutError" ? new Error("The request took too long") : error;
  }

  // An error carries its message in the body when it has one, and need not have one.
  const body = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(body?.error || `${response.status} ${response.statusText}`);
  }
  // A success that does not parse is a broken answer, not an empty one. It used to become
  // `{}`, and the caller then read a field off it and threw a TypeError three files from
  // the request that caused it - which on the chain list left the page with no retry.
  if (body === null || typeof body !== "object") {
    throw new Error("The server answered with something that is not JSON");
  }
  return body;
}
