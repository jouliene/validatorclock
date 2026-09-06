// The empty string is how this file says "nothing to show", and formatTokenAmount turns it
// into a dash. A total of zero is not nothing - a round in which nobody was rewarded yet has
// a reward total of 0 - so it is only nothing when no item carried a number at all.
function sumTokenValues(items, key) {
  const values = items
    .map((item) => item[key])
    .filter((value) => value !== undefined && value !== null && value !== "")
    .map(Number)
    .filter(Number.isFinite);
  return values.length ? String(values.reduce((sum, value) => sum + value, 0)) : "";
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
