function renderDateStack(container, start, end) {
  container.replaceChildren();
  container.append(dateRow("Start", start), dateRow("End", end));
}

function dateRow(label, unixSeconds) {
  const row = document.createElement("div");
  row.className = "date-row";
  const labelNode = document.createElement("span");
  labelNode.className = "date-label";
  labelNode.textContent = label;
  const valueNode = document.createElement("span");
  valueNode.className = "date-value";
  valueNode.textContent = formatDateTime(unixSeconds);
  row.append(labelNode, valueNode);
  return row;
}

function renderInfoUpdated(labelContainer, valueContainer, receivedAt, now, options = {}) {
  if (options.refreshing) {
    labelContainer.textContent = "Updating";
    valueContainer.textContent = "now";
    return;
  }

  labelContainer.textContent = "Info updated";
  if (receivedAt == null) {
    valueContainer.textContent = "waiting";
    return;
  }
  const elapsedMs = Math.max(0, now - receivedAt);
  valueContainer.textContent = elapsedMs < 250 ? "now" : `${Math.floor(elapsedMs / 1000)}s`;
}

function formatDateTime(unixSeconds) {
  if (!unixSeconds) {
    return "-";
  }
  const date = new Date(unixSeconds * 1000);
  const pad = (value) => String(value).padStart(2, "0");
  return [
    date.getFullYear(),
    pad(date.getMonth() + 1),
    pad(date.getDate()),
  ].join("-") + ` ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

function formatDuration(totalSeconds) {
  const seconds = Math.max(0, Math.trunc(totalSeconds));
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;

  if (days > 0) {
    return `${days}d ${hours}h`;
  }
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  if (minutes > 0) {
    return `${minutes}m ${remainder}s`;
  }
  return `${remainder}s`;
}


function formatDurationPrecise(totalSeconds) {
  const seconds = Math.max(0, Math.trunc(totalSeconds));
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;
  const parts = [];

  if (days > 0) {
    parts.push(`${days}d`);
  }
  if (hours > 0 || days > 0) {
    parts.push(`${hours}h`);
  }
  if (minutes > 0 || hours > 0 || days > 0) {
    parts.push(`${minutes}m`);
  }
  parts.push(`${remainder}s`);
  return parts.join(" ");
}
