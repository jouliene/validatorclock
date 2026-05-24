function sumTokenValues(items, key) {
  const total = items.reduce((sum, item) => {
    const value = Number(item[key] || 0);
    return Number.isFinite(value) ? sum + value : sum;
  }, 0);
  return total ? String(total) : "";
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
