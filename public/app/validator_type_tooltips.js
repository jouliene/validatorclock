function isFakeMapValidator(validator, options = {}) {
  if (!validatorMapAvailableForChain(options.chainId)) {
    return false;
  }
  if (!(options.fakeValidatorPeers instanceof Set)) {
    return false;
  }

  const publicKey = String(validator?.public_key || "").toLowerCase();
  return Boolean(publicKey) && options.fakeValidatorPeers.has(publicKey);
}

function fakeValidatorTypeTooltipLines(typeLabel, tooltipLines) {
  return Array.isArray(tooltipLines) && tooltipLines.length > 0
    ? tooltipLines
    : validatorTypeTooltipLines(typeLabel);
}

function validatorTypeTooltipLines(label, contractHash = "") {
  const entry = validatorTypeGlossaryEntry(label);
  const hash = String(contractHash || "").trim();
  const normalizedLabel = String(label || "").trim();
  const name = entry?.name || (normalizedLabel && normalizedLabel !== UNKNOWN_VALIDATOR_TYPE.label
    ? normalizedLabel
    : "Unknown");
  return [
    `Contract HASH: ${hash || "Unknown"}`,
    `Name: ${name}`,
  ];
}
