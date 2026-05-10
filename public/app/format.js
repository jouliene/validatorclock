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
  if (!address.includes(":")) {
    return shortenTonFriendlyAddress(address);
  }
  const [workchain, hash] = address.split(":");
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

function sumTokenValues(items, key) {
  const total = items.reduce((sum, item) => {
    const value = Number(item[key] || 0);
    return Number.isFinite(value) ? sum + value : sum;
  }, 0);
  return total ? String(total) : "";
}

function formatMasterchainAddress(hash) {
  if (!hash) {
    return "-";
  }
  return hash.includes(":") || !isHexHash(hash) ? hash : `-1:${hash}`;
}

function formatDisplayAddress(address, options = {}) {
  const raw = formatMasterchainAddress(address || "");
  if (!raw || raw === "-") {
    return { text: "-", value: "-", title: "-" };
  }

  if (options.addressType === "ton") {
    const friendly = toTonUserFriendlyAddress(raw);
    if (friendly) {
      return {
        text: shortenAddress(friendly),
        value: friendly,
        title: `${friendly} · ${raw}`,
      };
    }
  }

  return {
    text: shortenAddress(raw),
    value: raw,
    title: raw,
  };
}

function shortenTonFriendlyAddress(address) {
  if (!address) {
    return "-";
  }
  return shortenHash(address, 4, 4);
}

function toTonUserFriendlyAddress(address) {
  const raw = formatMasterchainAddress(address || "");
  if (!raw.includes(":")) {
    return "";
  }

  const [workchainPart, hash] = raw.split(":");
  if (!isHexHash(hash)) {
    return "";
  }

  const body = new Uint8Array(34);
  body[0] = 0x11;
  body[1] = tonWorkchainByte(Number(workchainPart));
  body.set(hexToBytes(hash), 2);

  const checksum = crc16Ccitt(body);
  const full = new Uint8Array(36);
  full.set(body);
  full[34] = checksum >> 8;
  full[35] = checksum & 0xff;
  return base64UrlEncode(full);
}

function tonWorkchainByte(workchain) {
  if (!Number.isFinite(workchain)) {
    return 0xff;
  }
  return workchain < 0 ? (256 + workchain) & 0xff : workchain & 0xff;
}

function isHexHash(value) {
  return /^[0-9a-fA-F]{64}$/.test(value || "");
}

function hexToBytes(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < hex.length; index += 2) {
    bytes[index / 2] = Number.parseInt(hex.slice(index, index + 2), 16);
  }
  return bytes;
}

function crc16Ccitt(bytes) {
  let crc = 0;
  for (const byte of bytes) {
    crc ^= byte << 8;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = crc & 0x8000 ? ((crc << 1) ^ 0x1021) & 0xffff : (crc << 1) & 0xffff;
    }
  }
  return crc;
}

function base64UrlEncode(bytes) {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  const base64 = typeof btoa === "function"
    ? btoa(binary)
    : Buffer.from(bytes).toString("base64");
  return base64.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
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
  return formatTokenAmount(value, 0, 0);
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
