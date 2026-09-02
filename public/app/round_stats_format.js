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
  // An amount that is simply not there used to come back as 0: Number("") is
  // 0, so the display string never failed and the raw fallback was never
  // reached. A round with no data was then charted as a real zero.
  const text = typeof display === "string" ? display.replace(/,/g, "").trim() : "";
  const displayNumber = text === "" ? Number.NaN : Number(text);
  if (Number.isFinite(displayNumber)) {
    return displayNumber;
  }
  return roundStatsNumber(raw);
}

/// A value only counts as a number when there is one there. Number(null) and
/// Number("") are both 0, and 0 passes every finite check downstream.
function roundStatsNumber(value) {
  if (value === null || value === undefined || value === "") {
    return Number.NaN;
  }
  const number = Number(value);
  return Number.isFinite(number) ? number : Number.NaN;
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
  // Grouped from the digits themselves. Parsing to a number first rewrote the
  // low digits of anything past 2^53 and cut the fraction short - in the one
  // place that promises the exact amount.
  const text = String(value).replace(/,/g, "").trim();
  const parts = /^(-?)(\d+)(\.\d+)?$/.exec(text);
  if (!parts) {
    return String(value);
  }
  const [, sign, whole, fraction] = parts;
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return `${sign}${grouped}${fraction || ""}`;
}

function formatRoundStatsExactPercent(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) {
    return "-";
  }
  return `${number.toFixed(2)}%`;
}

function roundStatsFinite(value) {
  return Number.isFinite(roundStatsNumber(value));
}
