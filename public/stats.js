(function () {
  const PUBLIC_ENDPOINT = "/api/analytics/public";
  const VISITORS_ENDPOINT = "/stats/visitors";
  const REFRESH_MS = 30_000;
  const HEARTBEAT_MS = 30_000;
  const FETCH_TIMEOUT_MS = 15_000;

  const COLUMNS = [
    { key: "ip", label: "IP address", kind: "text", className: "stats-cell-ip" },
    { key: "country", label: "Country", kind: "text" },
    { key: "city", label: "City", kind: "text" },
    { key: "isp", label: "Provider", kind: "text" },
    { key: "today_visits", label: "Today", kind: "number" },
    { key: "last_30_days_visits", label: "Last 30 days", kind: "number" },
    { key: "total_visits", label: "All time", kind: "number" },
    { key: "last_seen", label: "Last seen", kind: "time" },
    { key: "online", label: "On site", kind: "flag" },
  ];

  const state = {
    visitors: [],
    generatedAt: 0,
    filter: "",
    sortKey: "last_seen",
    sortDescending: true,
    authExpired: false,
  };

  function boot() {
    renderHeader();
    setupFilter();
    sendAnalyticsEvent("page_open");
    refresh();
    window.setInterval(refresh, REFRESH_MS);
    window.setInterval(sendVisibleAnalyticsHeartbeat, HEARTBEAT_MS);
    document.addEventListener("visibilitychange", handleVisibility);
  }

  function handleVisibility() {
    if (document.visibilityState !== "visible") {
      return;
    }
    sendAnalyticsEvent("heartbeat");
    refresh();
  }

  async function refresh() {
    await Promise.all([refreshSummary(), refreshVisitors()]);
  }

  async function refreshSummary() {
    const stats = await fetchJson(PUBLIC_ENDPOINT);
    if (!stats || !stats.today || !stats.last_30_days || !stats.all_time) {
      return;
    }

    setText("statsTodayVisits", formatAnalyticsNumber(stats.today.visits));
    setText(
      "statsTodayDetail",
      `${pluralize(stats.today.visits, "visit")} · ${pluralize(
        stats.today.unique_visitors,
        "unique visitor",
      )}`,
    );
    setText("statsMonthVisits", formatAnalyticsNumber(stats.last_30_days.visits));
    setText(
      "statsMonthDetail",
      `${pluralize(stats.last_30_days.visits, "visit")} · ${pluralize(
        stats.last_30_days.unique_visitors,
        "unique visitor",
      )}`,
    );
    setText("statsAllTimeVisits", formatAnalyticsNumber(stats.all_time.visits));
    setText("statsAllTimeDetail", pluralize(stats.all_time.visits, "visit"));
  }

  async function refreshVisitors() {
    const payload = await fetchJson(VISITORS_ENDPOINT);
    if (!payload || !Array.isArray(payload.visitors)) {
      return;
    }

    state.visitors = payload.visitors;
    state.generatedAt = Number(payload.generated_at) || 0;
    setText("statsOnlineNow", formatAnalyticsNumber(payload.online_now));
    setText("statsKnownVisitors", `${pluralize(payload.known_visitors, "address")} seen`);
    setText("statsUpdated", formatClock(state.generatedAt));
    renderRows();
  }

  async function fetchJson(url) {
    try {
      const response = await fetch(url, {
        headers: { Accept: "application/json" },
        cache: "no-store",
        credentials: "same-origin",
        // A hung request would otherwise leave the table stuck on its last
        // reading with no sign that refreshing had stopped.
        signal:
          typeof AbortSignal !== "undefined" && AbortSignal.timeout
            ? AbortSignal.timeout(FETCH_TIMEOUT_MS)
            : undefined,
      });
      if (response.status === 401 || response.status === 403) {
        state.authExpired = true;
        renderRows();
        return null;
      }
      if (!response.ok) {
        return null;
      }
      state.authExpired = false;
      return await response.json();
    } catch (_) {
      return null;
    }
  }

  function setupFilter() {
    const input = document.getElementById("statsFilter");
    if (!input) {
      return;
    }
    input.addEventListener("input", () => {
      state.filter = input.value.trim().toLowerCase();
      renderRows();
    });
  }

  function renderHeader() {
    const header = document.getElementById("statsTableHeader");
    if (!header) {
      return;
    }

    header.replaceChildren();
    for (const column of COLUMNS) {
      const cell = el(
        "button",
        {
          className: `stats-cell stats-head-cell ${cellClass(column)}`,
          attrs: { type: "button", role: "columnheader" },
          dataset: { key: column.key },
        },
        [
          el("span", { text: column.label }),
          el("span", { className: "stats-sort-marker", text: sortMarker(column.key) }),
        ],
      );

      cell.addEventListener("click", () => {
        if (state.sortKey === column.key) {
          state.sortDescending = !state.sortDescending;
        } else {
          state.sortKey = column.key;
          state.sortDescending = column.kind !== "text";
        }
        renderHeader();
        renderRows();
      });

      header.append(cell);
    }
  }

  function sortMarker(key) {
    if (state.sortKey !== key) {
      return "";
    }
    return state.sortDescending ? "▾" : "▴";
  }

  function renderRows() {
    const body = document.getElementById("statsTableBody");
    const empty = document.getElementById("statsEmpty");
    if (!body) {
      return;
    }

    const rows = sortVisitors(state.visitors.filter(matchesFilter));
    body.replaceChildren();
    for (const visitor of rows) {
      body.append(buildRow(visitor));
    }

    setText("statsRowCount", `${pluralize(rows.length, "address")} shown`);
    if (empty) {
      empty.hidden = rows.length > 0 && !state.authExpired;
      empty.textContent = emptyMessage(rows.length);
    }
  }

  function emptyMessage(rowCount) {
    if (state.authExpired) {
      return "Sign-in expired. Reload the page and enter the password again.";
    }
    if (state.visitors.length === 0) {
      return "No visitors recorded yet.";
    }
    return rowCount === 0 ? "No addresses match the filter." : "";
  }

  function buildRow(visitor) {
    return el(
      "div",
      {
        className: visitor.online ? "stats-row is-online" : "stats-row",
        attrs: { role: "row" },
      },
      COLUMNS.map((column) =>
        el(
          "div",
          {
            className: `stats-cell ${cellClass(column)}`,
            attrs: { role: "cell" },
            dataset: { label: column.label },
          },
          [buildCellContent(column, visitor)],
        ),
      ),
    );
  }

  function buildCellContent(column, visitor) {
    const value = visitor[column.key];
    if (column.kind === "flag") {
      return el("span", {
        className: value ? "stats-flag is-yes" : "stats-flag is-no",
        text: value ? "yes" : "no",
      });
    }
    if (column.kind === "number") {
      return document.createTextNode(formatAnalyticsNumber(value));
    }
    if (column.kind === "time") {
      return document.createTextNode(formatRelative(value));
    }
    if (column.key === "country") {
      return document.createTextNode(formatCountry(visitor));
    }
    return document.createTextNode(textValue(value));
  }

  function formatCountry(visitor) {
    const country = textValue(visitor.country);
    const code = typeof visitor.country_code === "string" ? visitor.country_code.trim() : "";
    if (code && country !== "-" && country.toUpperCase() !== code.toUpperCase()) {
      return `${country} (${code})`;
    }
    return country;
  }

  function matchesFilter(visitor) {
    if (!state.filter) {
      return true;
    }
    return [visitor.ip, visitor.country, visitor.country_code, visitor.city, visitor.isp, visitor.asn]
      .filter((value) => typeof value === "string")
      .some((value) => value.toLowerCase().includes(state.filter));
  }

  function sortVisitors(visitors) {
    const column = COLUMNS.find((item) => item.key === state.sortKey) || COLUMNS[0];
    const direction = state.sortDescending ? -1 : 1;
    return visitors.slice().sort((left, right) => {
      const result = compareValues(column, left, right);
      if (result !== 0) {
        return result * direction;
      }
      return left.ip.localeCompare(right.ip);
    });
  }

  function compareValues(column, left, right) {
    if (column.kind === "number" || column.kind === "time") {
      return (Number(left[column.key]) || 0) - (Number(right[column.key]) || 0);
    }
    if (column.kind === "flag") {
      return (left[column.key] ? 1 : 0) - (right[column.key] ? 1 : 0);
    }
    return textValue(left[column.key]).localeCompare(textValue(right[column.key]));
  }

  function textValue(value) {
    if (typeof value !== "string") {
      return "-";
    }
    const trimmed = value.trim();
    return trimmed === "" ? "-" : trimmed;
  }

  function pluralize(value, noun) {
    const number = Number(value);
    const safe = Number.isFinite(number) && number >= 0 ? number : 0;
    const plural = noun.endsWith("s") ? `${noun}es` : `${noun}s`;
    return `${formatAnalyticsNumber(safe)} ${safe === 1 ? noun : plural}`;
  }

  function formatRelative(value) {
    const seconds = Number(value);
    if (!Number.isFinite(seconds) || seconds <= 0) {
      return "-";
    }
    const reference = state.generatedAt || Math.floor(Date.now() / 1000);
    const delta = Math.max(0, reference - seconds);
    if (delta < 60) {
      return "just now";
    }
    if (delta < 3600) {
      return `${Math.floor(delta / 60)} min ago`;
    }
    if (delta < 86_400) {
      return `${Math.floor(delta / 3600)} h ago`;
    }
    return `${Math.floor(delta / 86_400)} d ago`;
  }

  function formatClock(seconds) {
    const value = Number(seconds);
    if (!Number.isFinite(value) || value <= 0) {
      return "-";
    }
    return new Date(value * 1000).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }

  function cellClass(column) {
    const classes = [`stats-col-${column.key.replace(/_/g, "-")}`];
    if (column.kind === "number") {
      classes.push("is-numeric");
    }
    if (column.className) {
      classes.push(column.className);
    }
    return classes.join(" ");
  }

  function setText(id, value) {
    const element = document.getElementById(id);
    if (element) {
      element.textContent = value;
    }
  }

  boot();
})();
