function roundStatsAxisLabel(value, unit) {
  if (!roundStatsFinite(value)) {
    return "-";
  }
  if (unit === "percent") {
    return `${value.toFixed(Math.abs(value) >= 10 ? 1 : 2)}%`;
  }
  if (unit === "count") {
    return String(Math.round(value));
  }
  return compactRoundStatsAmount(value);
}

function compactRoundStatsAmount(value) {
  const abs = Math.abs(value);
  if (abs >= 1_000_000_000) {
    return `${(value / 1_000_000_000).toFixed(1)}B`;
  }
  if (abs >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(1)}M`;
  }
  if (abs >= 1_000) {
    return `${(value / 1_000).toFixed(1)}K`;
  }
  return value.toFixed(abs >= 10 ? 0 : 2);
}

function roundStatsAmount(display, raw) {
  const displayNumber = Number(String(display || "").replace(/,/g, ""));
  if (Number.isFinite(displayNumber)) {
    return displayNumber;
  }
  const rawNumber = Number(raw);
  return Number.isFinite(rawNumber) ? rawNumber : NaN;
}

function formatRoundStatsPercent(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) {
    return "-";
  }
  return `${number.toFixed(2)}%`;
}

function formatRoundStatsExactAmount(value) {
  if (!value && value !== 0) {
    return "-";
  }
  const number = Number(String(value).replace(/,/g, ""));
  if (!Number.isFinite(number)) {
    return String(value);
  }
  return number.toLocaleString(undefined, {
    minimumFractionDigits: 0,
    maximumFractionDigits: 9,
  });
}

function formatRoundStatsExactPercent(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) {
    return "-";
  }
  return `${number.toFixed(2)}%`;
}

function roundStatsFinite(value) {
  return Number.isFinite(Number(value));
}
