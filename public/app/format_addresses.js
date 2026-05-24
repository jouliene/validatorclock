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

function formatMasterchainAddress(hash) {
  if (!hash) {
    return "-";
  }
  return hash.includes(":") || !isHexHash(hash) ? hash : `-1:${hash}`;
}

function formatDisplayAddress(address, options = {}) {
  const raw = formatMasterchainAddress(address || "");
  if (!raw || raw === "-") {
    return { text: "-", value: "-", title: "-", tooltip: [] };
  }

  const friendly = options.chainId === "ton" ? toTonUserFriendlyAddress(raw) : "";
  const tooltip = addressTooltipLines(raw, friendly);

  if (options.addressType === "ton") {
    if (friendly) {
      return {
        text: shortenAddress(friendly),
        value: friendly,
        title: tooltip.join("\n"),
        tooltip,
      };
    }
  }

  return {
    text: shortenAddress(raw),
    value: raw,
    title: tooltip.join("\n"),
    tooltip,
  };
}

function addressTooltipLines(raw, friendly) {
  const lines = [];
  if (friendly) {
    lines.push(`TON address: ${friendly}`);
  }
  lines.push(`EVER address: ${raw}`);
  return lines;
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
