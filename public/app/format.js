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

function renderInfoUpdated(container, fetchedAt, now) {
  const ageSeconds = Math.max(0, now - fetchedAt);
  container.textContent = `${ageSeconds}s ago`;
}

function validatorWalletAddress(validator) {
  const hash = validator.wallet;
  if (!hash) {
    return "-";
  }
  return formatMasterchainAddress(hash);
}

function shortenAddress(address) {
  if (!address || address === "-") {
    return "-";
  }
  const [workchain, hash] = address.includes(":") ? address.split(":") : ["-1", address];
  if (!hash) {
    return "-";
  }
  return `${workchain}:${hash.slice(0, 4)}...${hash.slice(-4)}`;
}

function shortenHash(value, head = 5, tail = 5) {
  if (!value) {
    return "-";
  }
  return value.length <= head + tail + 3 ? value : `${value.slice(0, head)}...${value.slice(-tail)}`;
}

function validatorGradient(seed) {
  const value = String(seed || "");
  let hash = 0;
  for (let i = 0; i < value.length; i += 1) {
    hash = (hash * 31 + value.charCodeAt(i)) >>> 0;
  }
  const gradients = [
    "linear-gradient(135deg, #67b7c7 0%, #2e6f8f 100%)",
    "linear-gradient(135deg, #75bd91 0%, #2f7655 100%)",
    "linear-gradient(135deg, #caa85c 0%, #806a36 100%)",
    "linear-gradient(135deg, #8f98c9 0%, #536093 100%)",
    "linear-gradient(135deg, #9d80ae 0%, #654a73 100%)",
    "linear-gradient(135deg, #c48771 0%, #81503f 100%)",
  ];
  return gradients[hash % gradients.length];
}

function sumTokenValues(items, key) {
  const total = items.reduce((sum, item) => {
    const value = Number(item[key] || 0);
    return Number.isFinite(value) ? sum + value : sum;
  }, 0);
  return total ? String(total) : "";
}

function formatMasterchainAddress(hash) {
  return hash.includes(":") ? hash : `-1:${hash}`;
}

function formatWeight(value) {
  return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

function formatPercent(value) {
  return `${Number(value || 0).toFixed(2)}%`;
}

function formatStakeAmount(value) {
  return formatTokenAmount(value, 0, 0);
}

function formatRewardAmount(value) {
  return formatTokenAmount(value, 0, 2);
}

function formatRewardCellAmount(value) {
  return formatTokenAmount(value, 0, 0);
}

function formatTokenAmount(value, minimumFractionDigits = 0, maximumFractionDigits = 3) {
  if (!value && value !== 0) {
    return "-";
  }
  const number = Number(value);
  if (!Number.isFinite(number)) {
    return value;
  }
  return number.toLocaleString(undefined, { minimumFractionDigits, maximumFractionDigits });
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

function formatDurationClock(totalSeconds) {
  const seconds = Math.max(0, Math.trunc(totalSeconds));
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;
  const pad = (value) => String(value).padStart(2, "0");

  if (days > 0) {
    return `${days}d ${pad(hours)}h ${pad(minutes)}m ${pad(remainder)}s`;
  }
  return `${pad(hours)}h ${pad(minutes)}m ${pad(remainder)}s`;
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
